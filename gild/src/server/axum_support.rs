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
use std::{borrow::Cow, collections::HashMap, sync::Arc};
use tracing::error;
use validator::{ValidationError, ValidationErrors, ValidationErrorsKind};

pub(crate) type Result<T> = core::result::Result<T, AppError>;

fn inner_validation_error(value: &Vec<ValidationError>) -> String {
	let mut msg = Vec::new();
	for item in value {
		msg.push(match item.code.to_string().as_str() {
			"length" => {
				let mut max = "none".to_string();
				let mut min = "0".to_string();

				for (field_k, field_v) in &item.params {
					if field_k == "max" {
						max = field_v.to_string()
					}
					if field_k == "min" {
						min = field_v.to_string()
					}
				}

				format!(
					"length is invalid: it must be between {} and {} characters",
					min, max
				)
			}
			"email" => "email address does not look valid".to_string(),
			_ => {
				let mut inner_msg = Vec::new();

				for (field_k, field_v) in &item.params {
					if field_k != "value" {
						inner_msg.push(format!("{}: {}", field_k, field_v.to_string()));
					}
				}

				format!(
					"type: {}, constraints: [{}]",
					item.code.to_string(),
					inner_msg.join(", ")
				)
			}
		})
	}

	msg.join(", ")
}

fn field_validation_error(value: HashMap<Cow<'static, str>, &Vec<ValidationError>>) -> String {
	let mut msg = Vec::new();

	for (k, v) in value {
		msg.push(format!("{}: [{}]", k, inner_validation_error(v)))
	}

	msg.join(" ")
}

fn human_validation_error(value: &HashMap<Cow<'static, str>, ValidationErrorsKind>) -> String {
	let mut msg = Vec::new();
	for (k, v) in value {
		let mut inner_msg = Vec::new();

		match v {
			ValidationErrorsKind::List(list) => {
				for (_, error) in list {
					inner_msg.push(field_validation_error(error.field_errors()))
				}
			}
			ValidationErrorsKind::Field(field) => inner_msg.push(inner_validation_error(field)),
			ValidationErrorsKind::Struct(s) => {
				inner_msg.push(field_validation_error(s.field_errors()));
			}
		}
		msg.push(format!(
			"{}: [{}]",
			if k == "plaintext_password" {
				"password"
			} else {
				k
			},
			inner_msg.join(", ")
		));
	}

	msg.join(", ")
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AppError(pub ProblemDetails);

impl<E> From<E> for AppError
where
	E: Into<anyhow::Error> + std::fmt::Debug + std::fmt::Display + Send + Sync + 'static,
{
	fn from(value: E) -> Self {
		let value: anyhow::Error = value.into();

		if value.is::<tonic::Status>() {
			let value = value.downcast_ref::<tonic::Status>().unwrap();
			return Self(
				ProblemDetails::new()
					.with_detail(value.message())
					.with_title("API sub-services error"),
			);
		}

		if value.is::<ValidationErrors>() {
			let value = value.downcast_ref::<ValidationErrors>().unwrap();

			return Self(
				ProblemDetails::new()
					.with_detail(human_validation_error(value.errors()))
					.with_title("Validation Error"),
			);
		}

		if value.is::<ProblemDetails>() {
			let value = value.downcast::<ProblemDetails>().unwrap();
			return Self(value);
		}

		Self(
			ProblemDetails::new()
				.with_detail(value.to_string())
				.with_title("Uncategorized Error"),
		)
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
			return AppError::from(anyhow!(e)).into_response();
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
				Err(e) => Err(AppError::from(anyhow!(e))),
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
				Err(e) => Err(AppError::from(anyhow!(e))),
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
			Err(AppError::from(anyhow!("user is not logged in")))
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
			log.with_error(&e.0);
		}

		let db = self.2.db.clone();

		tokio::spawn(async move { log.complete(&db).await.unwrap() });
		match self.0 {
			Ok(o) => o.into_response(),
			Err(e) => e.into_response(),
		}
	}
}
