use anyhow::{Result, anyhow};
use http::Uri;
use problem_details::ProblemDetails;
use serde::{Deserialize, Serialize};
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
#[welds(table = "audit_log")]
#[welds(BelongsTo(user, super::User, "user_id"))]
pub struct AuditLog {
	#[welds(primary_key)]
	pub id: u32,
	pub user_id: Option<u32>,
	pub time: chrono::DateTime<chrono::Local>,
	pub entry: String,
	pub endpoint: String,
	pub data: String,
	pub error: Option<String>,
}

// FIXME: I hate this lint but I guess it should be fixed eventually
#[allow(clippy::wrong_self_convention)]
impl AuditLog {
	pub fn builder() -> Self {
		Self::default()
	}

	pub fn from_uri(&mut self, uri: Uri) -> &mut Self {
		self.endpoint = uri.to_string();
		self
	}

	pub fn from_user(&mut self, user: &super::User) -> &mut Self {
		self.user_id = Some(user.id);
		self
	}

	pub fn with_error(&mut self, error: &ProblemDetails) -> &mut Self {
		self.error = Some(serde_json::to_string(error).unwrap());
		self
	}

	pub fn with_entry(&mut self, entry: &str) -> &mut Self {
		self.entry = entry.to_string();
		self
	}

	pub fn with_data<T>(&mut self, data: T) -> Result<&mut Self>
	where
		T: serde::Serialize,
	{
		self.data = serde_json::to_string(&data)?;
		Ok(self)
	}

	pub async fn complete(&mut self, db: &super::super::DB) -> Result<()> {
		let mut this = self.clone();
		this.time = chrono::Local::now();
		let mut state = DbState::new_uncreated(this);
		state
			.save(db.handle())
			.await
			.map_err(|e| anyhow!(e.to_string()))
	}
}
