use crate::zfs::Pool;
use anyhow::Result;
use serde::Deserialize;
use tracing::info;
use tracing_subscriber::FmtSubscriber;

pub(crate) const CONFIG_PATH: &str = "/trunk/config.yaml";
pub(crate) const DEFAULT_ZPOOL: &str = "trunk";

fn default_zpool() -> String {
	DEFAULT_ZPOOL.to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub enum LogLevel {
	#[serde(rename = "warn")]
	Warn,
	#[serde(rename = "info")]
	Info,
	#[serde(rename = "error")]
	Error,
	#[serde(rename = "debug")]
	Debug,
	#[serde(rename = "trace")]
	Trace,
}

impl From<LogLevel> for tracing::Level {
	fn from(value: LogLevel) -> Self {
		match value {
			LogLevel::Info => tracing::Level::INFO,
			LogLevel::Warn => tracing::Level::WARN,
			LogLevel::Error => tracing::Level::ERROR,
			LogLevel::Debug => tracing::Level::DEBUG,
			LogLevel::Trace => tracing::Level::TRACE,
		}
	}
}

impl From<tracing::Level> for LogLevel {
	fn from(value: tracing::Level) -> Self {
		match value {
			tracing::Level::INFO => LogLevel::Info,
			tracing::Level::WARN => LogLevel::Warn,
			tracing::Level::ERROR => LogLevel::Error,
			tracing::Level::DEBUG => LogLevel::Debug,
			tracing::Level::TRACE => LogLevel::Trace,
		}
	}
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
	pub socket: std::path::PathBuf,
	pub zfs: ZFSConfig,
	pub log_level: LogLevel,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ZFSConfig {
	#[serde(default = "default_zpool")]
	pub pool: String,
}

impl ZFSConfig {
	pub fn controller(&self) -> Pool {
		Pool::new(&self.pool)
	}
}

impl Config {
	pub fn from_file(filename: std::path::PathBuf) -> Result<Self> {
		let r = std::fs::OpenOptions::new().read(true).open(filename)?;
		let this: Self = serde_yaml_ng::from_reader(r)?;
		let subscriber = FmtSubscriber::builder()
			.with_max_level(Into::<tracing::Level>::into(this.log_level.clone()))
			.finish();
		tracing::subscriber::set_global_default(subscriber)?;
		info!("Configuration parsed successfully.");
		Ok(this)
	}
}

impl Default for Config {
	fn default() -> Self {
		Self::from_file(CONFIG_PATH.into()).expect("while reading config file")
	}
}
