use argon2::{
	Argon2,
	password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use validator::Validate;
use welds::WeldsModel;

use crate::db::DB;

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
#[welds(table = "users")]
#[welds(HasMany(sessions, super::Session, "user_id"))]
pub(crate) struct User {
	#[welds(primary_key)]
	#[welds(rename = "user_id")]
	#[serde(default = "u32::default")]
	pub id: u32,

	#[validate(length(min = 3, max = 30))]
	pub username: String,

	#[serde(skip_serializing_if = "Option::is_none")]
	#[validate(length(min = 3, max = 50))]
	pub realname: Option<String>,

	#[serde(skip_serializing_if = "Option::is_none")]
	#[validate(length(min = 6, max = 100), email)] // a@b.cd
	pub email: Option<String>,

	#[serde(skip_serializing_if = "Option::is_none")]
	#[validate(length(min = 10, max = 20))]
	pub phone: Option<String>,

	pub deleted_at: Option<chrono::DateTime<chrono::Local>>,

	#[welds(ignore)]
	// this should really skip totally, but is
	// needed for tests.
	#[serde(skip_serializing_if = "Option::is_none")]
	#[serde(rename = "password")]
	#[validate(length(min = 8, max = 100))]
	pub plaintext_password: Option<String>,

	#[serde(skip)]
	pub(crate) password: String,
}

impl User {
	pub(crate) fn login(&self, password: String) -> Result<()> {
		if self.deleted_at.is_some() {
			return Err(anyhow!("invalid login"));
		}

		let crypt = Argon2::default();
		let parsed = PasswordHash::new(&self.password).map_err(|e| anyhow!(e.to_string()))?;
		crypt
			.verify_password(password.as_bytes(), &parsed)
			.map_err(|e| anyhow!(e.to_string()))
	}

	pub(crate) fn set_password(&mut self, password: String) -> Result<()> {
		let crypt = Argon2::default();
		let salt = SaltString::generate(&mut OsRng);
		self.password = crypt
			.hash_password(password.as_bytes(), &salt)
			.map_err(|e| anyhow!(e.to_string()))?
			.to_string();
		Ok(())
	}

	pub async fn first_time_setup(db: &DB) -> Result<bool> {
		let count = User::all()
			.where_col(|c| c.deleted_at.equal(None))
			.count(db.handle())
			.await?;
		Ok(count == 0)
	}
}
