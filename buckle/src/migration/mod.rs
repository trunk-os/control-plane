#![allow(dead_code, unused_variables, unused_mut)]
use std::{collections::HashMap, pin::Pin, sync::Arc};
use thiserror::Error;
use tokio::sync::Mutex;

mod plans;
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
}

impl From<anyhow::Error> for MigrationError {
	fn from(value: anyhow::Error) -> Self {
		Self::UnknownWithMessage(value.to_string())
	}
}

pub type Migration = Vec<Box<dyn BoxedMigrationClosure>>;

pub async fn run_migrations<'a>(
	migrations: Migration, mut state: MigrationState,
) -> anyhow::Result<()> {
	for migration in migrations {
		let closure = migration.closure().await;
		let lock = closure.lock().await;
		state = (*lock)(state.clone()).await?;
	}

	Ok(())
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
	($name:ident, $state:ident, $func:block) => {
		Arc::new(Mutex::new(Box::new(|mut state: MigrationState| {
			let mut $state = state.clone();
			Box::pin(async move { $func })
		})))
	};
}

#[macro_export]
macro_rules! build_migration_set {
    ($state:ident, $(($name:ident, $func:block)),*) => {{
      let mut v: Migration = Vec::new();
      $(
      {
        v.push(Box::new(MigrationClosure(make_migration_func!($name, $state, $func))));
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
		let v = build_migration_set!(state, (foo, { Ok(state) }), (bar, { Ok(state) }));
		assert_eq!(v.len(), 2);
	}

	#[tokio::test]
	async fn test_run_migration() {
		let state: MigrationState = Default::default();
		let res = run_migrations(
			build_migration_set!(state, (foo, { Ok(state) }), (bar, { Ok(state) })),
			state,
		)
		.await;

		assert!(res.is_ok());

		let state: MigrationState = Default::default();
		let res = run_migrations(
			build_migration_set!(
				state,
				(foo, { Err(MigrationError::Unknown) }),
				(bar, { Ok(state) })
			),
			state,
		)
		.await;

		assert!(res.is_err())
	}

	#[tokio::test]
	async fn test_migration_state() {
		let state: MigrationState = Default::default();
		let res = run_migrations(
			build_migration_set!(
				state,
				(foo, {
					state.insert("hello".into(), "world".into());
					Ok(state)
				}),
				(bar, {
					if state.contains_key("hello") {
						Ok(state)
					} else {
						Err(MigrationError::Unknown)
					}
				})
			),
			state,
		)
		.await;

		assert!(res.is_ok());
	}
}
