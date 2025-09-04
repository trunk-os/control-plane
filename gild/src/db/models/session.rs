use super::{super::DB, User};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, ops::Deref};
use validator::Validate;
use welds::{WeldsModel, state::DbState};

#[derive(
	Debug,
	Clone,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	WeldsModel,
	Default,
	Serialize,
	Deserialize,
	Validate,
)]
#[welds(table = "sessions")]
#[welds(BelongsTo(user, User, "user_id"))]
pub(crate) struct Session {
	#[welds(primary_key)]
	#[welds(rename = "session_id")]
	pub id: u32,
	pub expires: chrono::DateTime<chrono::Local>,
	pub user_id: u32,
}

pub(crate) type JWTClaims = BTreeMap<String, String>;

pub(crate) const JWT_SESSION_ID_KEY: &str = "kid";
pub(crate) const JWT_EXPIRATION_TIME: &str = "exp";
pub(crate) const DEFAULT_EXPIRATION: i64 = 7;

impl Session {
	pub fn new_assigned(user: &User) -> DbState<Self> {
		DbState::new_uncreated(Self {
			user_id: user.id,
			expires: chrono::Local::now()
				.checked_add_signed(chrono::TimeDelta::days(
					DEFAULT_EXPIRATION,
				))
				.unwrap(),
			..Default::default()
		})
	}

	pub async fn prune(db: &DB) -> Result<()> {
		Self::all()
			.where_col(|c| {
				c.expires.lt(chrono::Local::now()
					- chrono::Duration::days(DEFAULT_EXPIRATION))
			})
			.delete(db.handle())
			.await?;
		Ok(())
	}

	pub(crate) async fn from_jwt(
		db: &DB, claims: JWTClaims,
	) -> Result<DbState<Self>> {
		let session_id: u32 = claims[JWT_SESSION_ID_KEY].parse()?;
		let list = Self::all()
			.where_col(|c| c.id.equal(session_id))
			.run(db.handle())
			.await?;
		let session = match list.first() {
			Some(inner) => inner.deref(),
			None => return Err(anyhow!("invalid session")),
		};

		let expires: chrono::DateTime<chrono::Local> =
			claims[JWT_EXPIRATION_TIME].parse()?;
		if session.expires.signed_duration_since(expires).num_seconds()
			< 0
		{
			return Err(anyhow!("session is expired"));
		}
		Ok(DbState::db_loaded(session.clone()))
	}

	pub(crate) fn to_jwt(&self) -> JWTClaims {
		let mut claims = JWTClaims::default();
		claims.insert(JWT_SESSION_ID_KEY.into(), self.id.to_string());
		claims.insert(
			JWT_EXPIRATION_TIME.into(),
			self.expires.to_rfc3339(),
		);
		claims
	}
}
