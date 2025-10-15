use std::{
	collections::{BTreeSet, HashMap},
	path::{Path, PathBuf},
	pin::Pin,
	process::ExitStatus,
};

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub(crate) const MIGRATION_FILENAME_TEMP: &str = "vayama.state.tmp";
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
// NOTE: the block you provide here will be executed with async coloring.
macro_rules! migration_func {
	($func:block) => {{ Box::new(|| Box::pin(async { $func })) }};
}

pub type MigrationAsyncFunc =
	Pin<Box<dyn 'static + Send + Future<Output = std::result::Result<(), MigrationError>>>>;

pub type MigrationFunc = Box<dyn FnMut() -> MigrationAsyncFunc>;

pub type MigrationRuntimeState = HashMap<String, String>;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationState {
	pub current_state: usize,
	pub failed_migrations: BTreeSet<String>,
}

#[derive(Debug, Default)]
pub struct Migrator {
	pub state_dir: PathBuf,
	pub state: MigrationState,
	pub runtime_state: MigrationRuntimeState,
	pub migrations: Vec<Migration>,
}

impl Migrator {
	pub fn new(migrations: Vec<Migration>, runtime_state: MigrationRuntimeState) -> Result<Self> {
		Self::new_with_root(migrations, runtime_state, None)
	}

	pub fn new_with_root(
		migrations: Vec<Migration>, runtime_state: MigrationRuntimeState,
		state_dir: Option<PathBuf>,
	) -> Result<Self> {
		let state_dir = state_dir.unwrap_or(PathBuf::from(DEFAULT_ROOT));
		let state_file = state_dir.join(MIGRATION_FILENAME);

		let state = match Self::state_from_file(&state_file) {
			Ok(state) => state,
			Err(_) => match Self::state_from_file(&state_dir.join(MIGRATION_FILENAME_TEMP)) {
				Ok(state) => state,
				Err(_) => Default::default(),
			},
		};

		let this = Self {
			state_dir,
			state,
			runtime_state,
			migrations,
		};

		this.persist_state()?;

		Ok(this)
	}

	pub fn state_from_file(state_file: &Path) -> Result<MigrationState> {
		let mut f = std::fs::OpenOptions::new().read(true).open(state_file)?;

		Ok(serde_json::from_reader(&mut f)?)
	}

	pub fn more_migrations(&self) -> bool {
		self.state.current_state < self.migrations.len()
	}

	pub fn persist_state(&self) -> Result<()> {
		let f = std::fs::OpenOptions::new()
			.write(true)
			.create(true)
			.truncate(true)
			.open(&self.state_dir.join(MIGRATION_FILENAME_TEMP))?;

		serde_json::to_writer(f, &self.state)?;

		Ok(std::fs::rename(
			self.state_dir.join(MIGRATION_FILENAME_TEMP),
			self.state_dir.join(MIGRATION_FILENAME),
		)?)
	}

	pub async fn execute_failed(&mut self) -> Result<()> {
		let failed_migrations = self.state.failed_migrations.clone();
		let migration_names: Vec<String> = self.migrations.iter().map(|x| x.name.clone()).collect();

		for (index, name) in migration_names.iter().enumerate() {
			if failed_migrations.contains(name) {
				let deps = self.migration_dependencies(index);
				match self.migrations[index]
					.execute(&self.state, deps, &self.runtime_state)
					.await
				{
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
		let migration = self.migrations.get_mut(self.state.current_state).unwrap();

		let orig = self.state.current_state;
		self.state.current_state += 1;

		return match migration
			.execute(&self.state, deps, &self.runtime_state)
			.await
		{
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
	// Create a struct by hand if you want anything more complicated; this is just to simplify
	// common cases.

	pub fn new(name: String, run: MigrationFunc) -> Self {
		Self {
			name,
			run,
			check: None,
			post_check: None,
			dependencies: Default::default(),
		}
	}

	pub fn new_with_check(name: String, run: MigrationFunc, check: MigrationFunc) -> Self {
		Self {
			name,
			run,
			check: Some(check),
			post_check: None,
			dependencies: Default::default(),
		}
	}

	pub async fn execute(
		&mut self, state: &MigrationState, dependencies: Vec<String>,
		_runtime_state: &MigrationRuntimeState,
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
