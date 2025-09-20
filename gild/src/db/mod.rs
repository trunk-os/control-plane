pub(crate) mod migrations;
pub(crate) mod models;
use crate::config::Config;
use anyhow::Result;
use sqlx::{Sqlite, migrate::MigrateDatabase};
use welds::connections::sqlite::{SqliteClient, connect};

#[derive(Clone)]
pub struct DB {
	handle: SqliteClient,
	filename: std::path::PathBuf,
}

impl std::fmt::Debug for DB {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(&format!(
			"db client, connected to {}",
			self.filename.to_str().unwrap()
		))
	}
}

pub async fn migrate(filename: std::path::PathBuf) -> Result<()> {
	migrations::migrate(filename).await
}

impl DB {
	pub async fn new(config: Config) -> Result<Self> {
		let this = match connect(&format!("sqlite:{}", config.db.to_str().unwrap())).await {
			Ok(c) => Self {
				handle: c,
				filename: config.db.clone(),
			},
			Err(_) => {
				Self::create(config.clone()).await?;
				Self {
					handle: connect(&format!("sqlite:{}", config.db.to_str().unwrap())).await?,
					filename: config.db.clone(),
				}
			}
		};
		migrate(this.filename.clone()).await?;
		Ok(this)
	}

	async fn create(config: Config) -> anyhow::Result<()> {
		if let Some(parent) = config.db.parent() {
			std::fs::create_dir_all(parent)?;
		}

		sqlx::sqlite::CREATE_DB_WAL.store(true, std::sync::atomic::Ordering::Release);

		Sqlite::create_database(&format!("sqlite:{}", config.db.to_str().unwrap())).await?;
		Ok(())
	}

	pub fn handle(&self) -> &SqliteClient {
		&self.handle
	}
}
