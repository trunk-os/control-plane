use super::ServerState;
use crate::db::models::{AuditLog, JWTClaims, Session, User};
use anyhow::anyhow;
use axum::{
	extract::{FromRequest, FromRequestParts, Path},
	http::{StatusCode, request::Parts},
	response::{IntoResponse, Response},
};
use axum_serde::Cbor;
use hmac::{Hmac, Mac};
use jwt::{Header, Token, Verified, VerifyWithKey};
use problem_details::ProblemDetails;
use std::{
	any::{Any, TypeId},
	sync::Arc,
};
use tracing::error;

pub(crate) type Result<T> = core::result::Result<T, AppError>;

#[derive(Debug, Clone, Default)]
pub(crate) struct AppError(pub ProblemDetails);

impl<E> From<E> for AppError
where
	E: Into<anyhow::Error> + Any,
{
	fn from(value: E) -> Self {
		// hack around type specialization
		if TypeId::of::<E>() == TypeId::of::<ProblemDetails>() {
			Self(
				<dyn Any + 'static>::downcast_ref::<ProblemDetails>(&value)
					.unwrap()
					.clone(),
			)
		} else if TypeId::of::<E>() == TypeId::of::<tonic::Status>() {
			Self(
				ProblemDetails::new()
					.with_detail(
						<dyn Any + 'static>::downcast_ref::<tonic::Status>(&value)
							.unwrap()
							.message(),
					)
					.with_title("Uncategorized Error"),
			)
		} else {
			Self(
				ProblemDetails::new()
					.with_detail(value.into().to_string())
					.with_title("Uncategorized Error"),
			)
		}
	}
}

impl IntoResponse for AppError {
	fn into_response(self) -> Response {
		self.0.into_response()
	}
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CborOut<T>(pub T);

impl<T> IntoResponse for CborOut<T>
where
	T: serde::Serialize,
{
	fn into_response(self) -> Response {
		let mut inner = Vec::with_capacity(65535);
		let mut buf = std::io::Cursor::new(&mut inner);

		if let Err(e) = ciborium::into_writer(&self.0, &mut buf) {
			return Into::<AppError>::into(anyhow!(e)).into_response();
		}

		Response::builder()
			.header("Content-Type", "application/cbor")
			.body(axum::body::Body::from(buf.into_inner().to_vec()))
			.unwrap()
	}
}

pub(crate) struct MyPath<T>(pub T);
impl<T> FromRequestParts<Arc<ServerState>> for MyPath<T>
where
	T: for<'de> serde::Deserialize<'de> + Send,
{
	type Rejection = AppError;

	fn from_request_parts(
		parts: &mut Parts, state: &Arc<ServerState>,
	) -> impl Future<Output = std::result::Result<Self, Self::Rejection>> + Send {
		async move {
			match Path::from_request_parts(parts, state).await {
				Ok(Path(x)) => Ok(MyPath(x)),
				Err(e) => Err(AppError::from(e)),
			}
		}
	}
}

pub(crate) struct MyCbor<T>(pub T);

impl<T> FromRequest<Arc<ServerState>> for MyCbor<T>
where
	T: for<'de> serde::Deserialize<'de>,
{
	type Rejection = AppError;

	fn from_request(
		req: axum::extract::Request, state: &Arc<ServerState>,
	) -> impl Future<Output = std::result::Result<Self, Self::Rejection>> + Send {
		async move {
			match Cbor::from_request(req, state).await {
				Ok(Cbor(x)) => Ok(MyCbor(x)),
				Err(e) => Err(AppError::from(e)),
			}
		}
	}
}

pub(crate) struct Account<T>(pub T);

async fn read_jwt(parts: &mut Parts, state: &Arc<ServerState>) -> Result<Option<User>> {
	// FIXME: we want to hide the error from the end user to avoid giving them information about this
	// process. We should, however, log the errors for debugging purposes, which isn't done yet.
	let err = AppError(
		ProblemDetails::new()
			.with_detail("Please enter correct credentials")
			.with_status(http::StatusCode::UNAUTHORIZED)
			.with_title("Invalid Login"),
	);

	let token = parts
		.headers
		.get(http::header::AUTHORIZATION)
		.ok_or(err.clone())?;

	let token = token
		.to_str()
		.map_err(|_| err.clone())?
		.strip_prefix("Bearer ")
		.unwrap();
	let signing_key: Hmac<sha2::Sha384> =
		Hmac::new_from_slice(&state.config.signing_key).map_err(|_| err.clone())?;

	let token: Token<Header, JWTClaims, Verified> = match token.verify_with_key(&signing_key) {
		Ok(x) => x,
		Err(e) => {
			error!("Error verifying token: {}", e);
			return Err(err);
		}
	};

	let session = match Session::from_jwt(&state.db, token.claims().clone()).await {
		Ok(x) => x,
		Err(e) => {
			error!("Error locating session from JWT: {}", e);
			return Err(err);
		}
	};

	match User::find_by_id(state.db.handle(), session.user_id).await {
		Ok(Some(user)) => {
			if user.deleted_at.is_none() {
				Ok(Some(user.into_inner()))
			} else {
				error!("User was deleted at {}", user.deleted_at.unwrap());
				Ok(None)
			}
		}
		Ok(None) => {
			error!(
				"User authenticated but not found: User ID: {}",
				session.user_id
			);
			Ok(None)
		}
		Err(e) => {
			error!("Error finding user: {}", e);
			Ok(None)
		}
	}
}

impl FromRequestParts<Arc<ServerState>> for Account<User> {
	type Rejection = AppError;

	async fn from_request_parts(
		parts: &mut Parts, state: &Arc<ServerState>,
	) -> core::result::Result<Self, Self::Rejection> {
		Session::prune(&state.db).await?; // prune sessions before trying to read them
		if let Some(user) = read_jwt(parts, state).await? {
			Ok(Account(user))
		} else {
			Err(anyhow!("user is not logged in").into())
		}
	}
}

impl FromRequestParts<Arc<ServerState>> for Account<Option<User>> {
	type Rejection = (StatusCode, &'static str);

	async fn from_request_parts(
		parts: &mut Parts, state: &Arc<ServerState>,
	) -> core::result::Result<Self, Self::Rejection> {
		Ok(Account(read_jwt(parts, state).await.unwrap_or_default()))
	}
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Log(pub(crate) AuditLog);

impl FromRequestParts<Arc<ServerState>> for Log {
	type Rejection = AppError;

	async fn from_request_parts(
		parts: &mut Parts, state: &Arc<ServerState>,
	) -> core::result::Result<Self, Self::Rejection> {
		let mut this = Self(AuditLog::builder().from_uri(parts.uri.clone()).clone());

		if let Some(user) = read_jwt(parts, state).await.unwrap_or_default() {
			this.0 = this.0.from_user(&user).clone();
		}

		Ok(this)
	}
}

pub async fn with_log<T>(
	state: Arc<ServerState>, log: &mut AuditLog,
	mut f: impl AsyncFnMut(Arc<ServerState>, &mut AuditLog) -> Result<T>,
) -> Result<WithLog<T>>
where
	T: IntoResponse,
{
	match f(state.clone(), log).await {
		Ok(res) => Ok(WithLog(Ok(res), log.clone(), state)),
		Err(e) => Ok(WithLog(Err(e), log.clone(), state)),
	}
}

#[derive(Debug, Clone)]
pub(crate) struct WithLog<T>(
	pub(crate) Result<T>,
	pub(crate) AuditLog,
	pub(crate) Arc<ServerState>,
);

impl<T> IntoResponse for WithLog<T>
where
	T: IntoResponse,
{
	fn into_response(self) -> Response {
		let mut log = self.1;
		if let Err(ref e) = self.0 {
			log = log.with_error(&e.0.to_string()).clone();
		}

		let db = self.2.db.clone();

		tokio::spawn(async move { log.complete(&db).await.unwrap() });
		match self.0 {
			Ok(o) => o.into_response(),
			Err(e) => e.into_response(),
		}
	}
}
