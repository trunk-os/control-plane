use buckle::client::Info;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Pagination {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<chrono::DateTime<chrono::Local>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_page: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u8>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct LogParameters {
    pub name: String,
    pub count: usize,
    pub cursor: Option<String>,
    pub direction: Option<buckle::systemd::LogDirection>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Token {
    pub(crate) token: String,
}

#[derive(Debug, Clone, Default, Validate, Serialize, Deserialize)]
pub struct Authentication {
    #[validate(length(min = 3, max = 30))]
    pub username: String,
    #[validate(length(min = 8, max = 100))]
    pub password: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PingResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<HealthStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<Info>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HealthStatus {
    pub buckle: Health,
    pub charon: Health,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Health {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromptResponsesWithName {
    pub name: String,
    pub responses: charon::PromptResponses,
}
