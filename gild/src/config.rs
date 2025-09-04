use anyhow::{Result, anyhow};
use rand::Fill;
use serde::Deserialize;
use std::net::SocketAddr;
use tracing::info;
use tracing_subscriber::FmtSubscriber;

const DEFAULT_BUCKLE_PATH: &str = "/tmp/buckled.sock";
const DEFAULT_CHARON_PATH: &str = "/tmp/charond.sock";
const DEFAULT_DB: &str = "/gild.db";
const DEFAULT_LISTEN: &str = "0.0.0.0:3000";

fn default_db() -> std::path::PathBuf {
	DEFAULT_DB.into()
}

fn default_buckle_socket() -> std::path::PathBuf {
	DEFAULT_BUCKLE_PATH.into()
}

fn default_charon_socket() -> std::path::PathBuf {
	DEFAULT_CHARON_PATH.into()
}

fn default_listen() -> SocketAddr {
	DEFAULT_LISTEN.parse().unwrap()
}

fn default_random() -> Vec<u8> {
	let mut v: [u8; 64] = [0u8; 64];
	v.fill(&mut rand::rng());
	v.to_vec()
}

#[derive(Debug, Clone, Deserialize)]
pub struct SocketConfig {
	#[serde(default = "default_buckle_socket")]
	pub buckle: std::path::PathBuf,
	#[serde(default = "default_charon_socket")]
	pub charon: std::path::PathBuf,
}

impl Default for SocketConfig {
	fn default() -> Self {
		Self {
			buckle: default_buckle_socket(),
			charon: default_charon_socket(),
		}
	}
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
	#[serde(default = "default_listen")]
	pub listen: SocketAddr,
	pub sockets: SocketConfig,
	#[serde(default = "default_db")]
	pub db: std::path::PathBuf,
	#[serde(default = "default_random")]
	pub signing_key: Vec<u8>,
	#[serde(default = "default_random")]
	pub signing_key_salt: Vec<u8>,
	pub log_level: buckle::config::LogLevel,
}

impl Default for Config {
	fn default() -> Self {
		let mut this = Self {
			listen: default_listen(),
			sockets: Default::default(),
			db: default_db(),
			signing_key: default_random(),
			signing_key_salt: default_random(),
			log_level: buckle::config::LogLevel::Info,
		};
		this.start_tracing().unwrap();
		this.convert_signing_key().unwrap();
		this
	}
}

impl Config {
	fn start_tracing(&self) -> Result<()> {
		let subscriber = FmtSubscriber::builder()
			.with_max_level(Into::<tracing::Level>::into(
				self.log_level.clone(),
			))
			.finish();
		tracing::subscriber::set_global_default(subscriber)?;

		info!("Configuration parsed");
		Ok(())
	}

	pub fn from_file(file: std::path::PathBuf) -> Result<Self> {
		let file = std::fs::OpenOptions::new().read(true).open(file)?;
		let mut this: Self = serde_yaml_ng::from_reader(file)?;
		this.start_tracing()?;
		this.convert_signing_key()?;
		Ok(this)
	}

	fn convert_signing_key(&mut self) -> Result<()> {
		let mut buf: [u8; 64] = [0u8; 64];
		let kdf = argon2::Argon2::default();
		kdf.hash_password_into(
			self.signing_key.as_slice(),
			self.signing_key_salt.as_slice(),
			&mut buf,
		)
		.map_err(|e| anyhow!(e.to_string()))?;

		// overwrite the keys in the config
		// we never generate (just parse) this format so this is a safe conversion.
		self.signing_key = buf.to_vec();
		self.signing_key_salt = Vec::default();

		Ok(())
	}

	pub(crate) async fn get_db(&self) -> Result<crate::db::DB> {
		crate::db::DB::new(self.clone()).await
	}

	pub(crate) fn buckle(&self) -> Result<buckle::client::Client> {
		buckle::client::Client::new(self.sockets.buckle.clone())
	}

	pub(crate) fn charon(&self) -> Result<charon::Client> {
		charon::Client::new(self.sockets.charon.clone())
	}
}
