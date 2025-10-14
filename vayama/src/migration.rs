use std::{collections::BTreeSet, path::PathBuf, pin::Pin, process::ExitStatus};

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub(crate) const MIGRATION_FILENAME: &str = "vayama.state";
pub(crate) const DEFAULT_ROOT: &str = "/etc";

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

#[macro_export]
macro_rules! migration_func {
	($func:block) => {{ Box::new(move || Box::pin(async move { $func })) }};
}

pub type MigrationAsyncFunc =
	Pin<Box<dyn 'static + Send + Future<Output = std::result::Result<(), MigrationError>>>>;

pub type MigrationFunc = Box<dyn FnMut() -> MigrationAsyncFunc>;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationState {
	pub current_state: usize,
	pub failed_migrations: BTreeSet<String>,
}

#[derive(Debug, Default)]
pub struct Migrator {
	pub state_file: PathBuf,
	pub state: MigrationState,
	pub migrations: Vec<Migration>,
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

		Ok(Self {
			state_file,
			state,
			migrations,
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

	pub async fn execute_failed(&mut self) -> Result<()> {
		let failed_migrations = self.state.failed_migrations.clone();
		let migration_names: Vec<String> = self.migrations.iter().map(|x| x.name.clone()).collect();

		for (index, name) in migration_names.iter().enumerate() {
			if failed_migrations.contains(name) {
				let deps = self.migration_dependencies(index);
				match self.migrations[index].execute(&self.state, deps).await {
					Ok(_) => {
						self.state.failed_migrations.remove(name);
					}
					Err(e) => {
						tracing::error!(
							"Error during vayama migration '{}'. Marking failed: {}",
							self.migrations[index].name,
							e
						);
					}
				}
			}
		}

		self.persist_state()?;
		Ok(())
	}

	pub fn migration_dependencies(&mut self, index: usize) -> Vec<String> {
		let mut deps = BTreeSet::new();

		for dep in &self.migrations[index].dependencies {
			deps.insert(dep.clone());
		}

		let mut last_len = 0;
		let mut current_len = deps.len();
		while last_len != current_len {
			last_len = current_len;
			// iterate each dependency and collect its dependencies. Stop processing when we know we
			// haven't processed anything new.
			//
			// NOTE: Do not attempt sort them in execution order
			// (tsort) as only existence is necessary for our work.
			for dep in deps.clone() {
				if let Some(m) = self.migrations.iter().find(|x| x.name == dep) {
					for dep in &m.dependencies {
						deps.insert(dep.clone());
					}
				}
			}

			current_len = deps.len();
		}

		deps.iter().map(Clone::clone).collect::<Vec<String>>()
	}

	pub async fn execute(&mut self) -> Result<Option<usize>> {
		if !self.more_migrations() {
			return Err(anyhow!(
				"current state ({}) is greater than migrations length ({})",
				self.state.current_state,
				self.migrations.len()
			));
		}

		let deps = self.migration_dependencies(self.state.current_state);
		if let Some(migration) = self.migrations.get_mut(self.state.current_state) {
			let orig = self.state.current_state;
			self.state.current_state += 1;

			return match migration.execute(&self.state, deps).await {
				Ok(_) => {
					self.persist_state()?;

					Ok(Some(orig))
				}
				Err(e) => {
					tracing::error!(
						"Error during vayama migration '{}'. Marking failed: {}",
						migration.name,
						e
					);

					self.state.failed_migrations.insert(migration.name.clone());
					self.persist_state()?;

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
		&mut self, state: &MigrationState, dependencies: Vec<String>,
	) -> std::result::Result<(), MigrationError> {
		for migration in &state.failed_migrations {
			if self.dependencies.contains(migration) || dependencies.contains(migration) {
				return Err(MigrationError::DependencyFailed(
					self.name.clone(),
					migration.clone(),
				));
			}
		}

		if let Some(check) = &mut self.check {
			if let Err(e) = check().await {
				return Err(e);
			}
		}

		let call = (self.run)();

		if let Err(e) = call.await {
			return Err(e);
		}

		if let Some(post_check) = &mut self.post_check {
			if let Err(e) = post_check().await {
				return Err(e);
			}
		}

		Ok(())
	}
}
