#[cfg(test)]
mod tests;

use std::{collections::BTreeMap, path::PathBuf, process::ExitStatus};

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const MIGRATION_FILENAME: &str = "vayama.state";
const DEFAULT_ROOT: &str = "/etc";

#[derive(Debug, Clone, Default, Error)]
pub enum MigrationError {
	#[default]
	#[error("Unknown error")]
	Unknown,
	#[error("Unknown error: {0}")]
	UnknownWithMessage(String),
	#[error("Cannot run migration {0}: dependency {1} failed")]
	DependencyFailed(String, String),
	#[error("Validation failed: {0}")]
	ValidationFailed(String),
	#[error("Command failed [status: {0}]: {1:?}: output: {2:?}")]
	CommandFailed(ExitStatus, Vec<String>, Option<String>),
}

pub type MigrationFunc = Box<dyn FnMut() -> std::result::Result<(), MigrationError>>;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MigrationState {
	pub current_state: usize,
	pub failed_migrations: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Migrator {
	pub state_file: PathBuf,
	pub state: MigrationState,
	pub migrations: BTreeMap<String, Migration>,
}

impl Migrator {
	pub fn new(migrations: Vec<Migration>) -> Result<Self> {
		Self::new_with_root(migrations, PathBuf::from(DEFAULT_ROOT))
	}

	pub fn new_with_root(migrations: Vec<Migration>, root: PathBuf) -> Result<Self> {
		let state_file = root.join(MIGRATION_FILENAME);

		let state: MigrationState = match std::fs::OpenOptions::new().read(true).open(&state_file) {
			Ok(mut f) => serde_json::from_reader(&mut f).unwrap_or_default(),
			Err(_) => MigrationState::default(),
		};

		let mut bt_migrations = BTreeMap::default();

		for migration in migrations {
			bt_migrations.insert(migration.name.clone(), migration);
		}

		Ok(Self {
			state_file,
			state,
			migrations: bt_migrations,
		})
	}

	pub fn more_migrations(&self) -> bool {
		self.state.current_state < self.migrations.len()
	}

	pub fn persist_state(&self) -> Result<()> {
		let f = std::fs::OpenOptions::new()
			.write(true)
			.create(true)
			.truncate(true)
			.open(&self.state_file)?;

		Ok(serde_json::to_writer(f, &self.state)?)
	}

	pub async fn execute(&mut self) -> Result<Option<usize>> {
		if !self.more_migrations() {
			return Err(anyhow!(
				"current state ({}) is greater than migrations length ({})",
				self.state.current_state,
				self.migrations.len()
			));
		}

		if let Some(migration) = self.migrations.values_mut().nth(self.state.current_state) {
			return match migration.execute(&self.state).await {
				Ok(_) => {
					self.persist_state()?;

					let orig = self.state.current_state;
					self.state.current_state += 1;

					Ok(Some(orig))
				}
				Err(e) => {
					tracing::error!(
						"Error during vayama migration '{}'. Marking failed: {}",
						migration.name,
						e
					);

					if !self.state.failed_migrations.contains(&migration.name) {
						self.state.failed_migrations.push(migration.name.clone());
						self.persist_state()?;
					}

					Ok(None)
				}
			};
		}

		Err(anyhow!("More migrations were reported but none were found"))
	}
}

pub struct Migration {
	pub name: String,
	pub dependencies: Vec<String>,
	pub check: Option<MigrationFunc>,
	pub run: MigrationFunc,
	pub post_check: Option<MigrationFunc>,
}

impl std::fmt::Debug for Migration {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(&format!("[vayama migration: {}]", self.name))
	}
}

impl Migration {
	pub async fn execute(
		&mut self, state: &MigrationState,
	) -> std::result::Result<(), MigrationError> {
		for migration in &state.failed_migrations {
			if self.dependencies.contains(migration) {
				return Err(MigrationError::DependencyFailed(
					self.name.clone(),
					migration.clone(),
				));
			}
		}

		if let Some(check) = &mut self.check {
			if let Err(e) = check() {
				return Err(e);
			}
		}

		if let Err(e) = (self.run)() {
			return Err(e);
		}

		if let Some(post_check) = &mut self.post_check {
			if let Err(e) = post_check() {
				return Err(e);
			}
		}

		Ok(())
	}
}
