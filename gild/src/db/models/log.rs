use anyhow::{anyhow, Result};
use http::{HeaderMap, HeaderValue, Uri};
use serde::{Deserialize, Serialize};
use validator::Validate;
use welds::{state::DbState, WeldsModel};

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
    pub ip: String,
    pub data: String,
    pub error: Option<String>,
}

impl AuditLog {
    pub fn builder() -> Self {
        Self::default()
    }

    pub fn from_uri(&mut self, uri: Uri) -> &mut Self {
        self.endpoint = uri.to_string();
        self
    }

    pub fn from_headers(&mut self, headers: HeaderMap<HeaderValue>) -> &mut Self {
        self.ip = headers
            .get("X-Real-IP")
            .map(|e| e.to_str().unwrap())
            .unwrap_or_else(|| {
                headers
                    .get("X-Forwarded-For")
                    .map(|e| e.to_str().unwrap().split("; ").next().unwrap_or_default())
                    .unwrap_or_else(|| "")
            })
            .to_string();
        self
    }

    pub fn from_user(&mut self, user: &super::User) -> &mut Self {
        self.user_id = Some(user.id);
        self
    }

    pub fn with_error(&mut self, error: &str) -> &mut Self {
        self.error = Some(error.to_string());
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
        Ok(state
            .save(db.handle())
            .await
            .map_err(|e| anyhow!(e.to_string()))?)
    }
}
