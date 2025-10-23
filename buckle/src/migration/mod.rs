#![allow(dead_code, unused_variables, unused_mut)]
use std::{
	collections::{HashMap, HashSet},
	path::PathBuf,
	pin::Pin,
	sync::Arc,
};
use thiserror::Error;
use tokio::sync::Mutex;

pub mod plans;
mod utils;

pub type MigrationState = HashMap<String, String>;
pub type MigrationResult = Result<MigrationState, MigrationError>;
pub type MigrationFunc = Arc<
	Mutex<
		Box<dyn Send + Fn(MigrationState) -> Pin<Box<dyn Send + Future<Output = MigrationResult>>>>,
	>,
>;

#[derive(Debug, Clone, Default, Error)]
pub enum MigrationError {
	#[default]
	#[error("Unknown Error")]
	Unknown,
	#[error("Unknown Error: {0}")]
	UnknownWithMessage(String),
	#[error("Error: [exit: {2}] [command: {0}]: {1}")]
	Command(String, String, i32),
	#[error("Error: [filename: {0}]: {1}")]
	WriteFile(PathBuf, String),
	#[error("Error launching command: [command: {0}]: {1}")]
	CommandLaunch(PathBuf, String),
}

impl From<anyhow::Error> for MigrationError {
	fn from(value: anyhow::Error) -> Self {
		Self::UnknownWithMessage(value.to_string())
	}
}

pub type Migration = Vec<Box<dyn BoxedMigrationClosure>>;

pub async fn run_migrations<'a>(
	map: HashMap<&'static str, Migration>, mut state: MigrationState,
) -> anyhow::Result<()> {
	let mut completed: HashSet<String> = match std::fs::OpenOptions::new()
		.read(true)
		.open("/trunk/.buckle-migrations.json")
	{
		Ok(mut f) => {
			let v: Vec<String> = serde_json::from_reader(&mut f)?;
			let mut map = HashSet::new();

			for s in v {
				map.insert(s);
			}

			map
		}
		Err(_) => HashSet::new(),
	};

	for (name, migration) in map {
		if completed.contains(name) {
			continue;
		}

		state = run_migration(migration, state.clone()).await?;
		completed.insert(name.to_string());

		let mut f = std::fs::OpenOptions::new()
			.write(true)
			.create(true)
			.truncate(true)
			.open("/trunk/.buckle-migrations.json.tmp")?;

		serde_json::to_writer(&mut f, &completed)?;
		drop(f);

		std::fs::rename(
			"/trunk/.buckle-migrations.json.tmp",
			"/trunk/.buckle-migrations.json",
		)?;
	}

	Ok(())
}

async fn run_migration(
	migrations: Migration, mut state: MigrationState,
) -> Result<MigrationState, MigrationError> {
	for migration in migrations {
		let closure = migration.closure().await;
		let lock = closure.lock().await;
		state = (*lock)(state.clone()).await?;
	}

	Ok(state)
}

#[async_trait::async_trait]
pub trait BoxedMigrationClosure {
	async fn call_with_state(&self, state: MigrationState) -> MigrationResult;
	async fn closure(&self) -> MigrationFunc;
}

#[derive(Clone)]
pub struct MigrationClosure(MigrationFunc);

#[async_trait::async_trait]
impl BoxedMigrationClosure for MigrationClosure {
	async fn call_with_state(&self, state: MigrationState) -> MigrationResult {
		let lock = self.0.lock().await;
		(*lock)(state).await
	}

	async fn closure(&self) -> MigrationFunc {
		self.0.clone()
	}
}

#[macro_export]
macro_rules! make_migration_func {
	($state:ident, $func:block) => {
		Arc::new(Mutex::new(Box::new(|mut state: MigrationState| {
			let mut $state = state.clone();
			Box::pin(async move { $func })
		})))
	};
}

#[macro_export]
macro_rules! build_migration_set {
    ($state:ident, $($func:block),*) => {{
      let mut v: Migration = Vec::new();
      $(
      {
        v.push(Box::new(MigrationClosure(make_migration_func!($state, $func))));
      }
      )*
      v
    }}
  }

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn test_build_migration_set() {
		let state: MigrationState = Default::default();
		let v = build_migration_set!(state, { Ok(state) }, { Ok(state) });
		assert_eq!(v.len(), 2);
	}

	#[tokio::test]
	async fn test_run_migration() {
		let state: MigrationState = Default::default();
		let res = run_migration(
			build_migration_set!(state, { Ok(state) }, { Ok(state) }),
			state,
		)
		.await;

		assert!(res.is_ok());

		let state: MigrationState = Default::default();
		let res = run_migration(
			build_migration_set!(state, { Err(MigrationError::Unknown) }, { Ok(state) }),
			state,
		)
		.await;

		assert!(res.is_err())
	}

	#[tokio::test]
	async fn test_migration_state() {
		let state: MigrationState = Default::default();
		let res = run_migration(
			build_migration_set!(
				state,
				{
					state.insert("hello".into(), "world".into());
					Ok(state)
				},
				{
					if state.contains_key("hello") {
						Ok(state)
					} else {
						Err(MigrationError::Unknown)
					}
				}
			),
			state,
		)
		.await;

		assert!(res.is_ok());
	}
}
