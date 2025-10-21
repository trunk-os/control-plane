#![allow(dead_code, unused_variables, unused_mut)]
use std::{collections::HashMap, pin::Pin, sync::Arc};
use thiserror::Error;
use tokio::sync::Mutex;

mod plans;

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

pub struct Migration<'a>(Vec<&'a dyn BoxedMigrationClosure>);

impl<'a> Migration<'a> {
	pub async fn run(&self, mut state: MigrationState) -> Result<(), MigrationError> {
		for func in &self.0 {
			let closure = func.closure().await;
			let lock = closure.lock().await;
			state = (*lock)(state).await?;
		}

		Ok(())
	}
}

pub async fn run_migrations(migrations: Vec<Migration<'_>>) -> anyhow::Result<()> {
	for migration in &migrations {
		migration.run(Default::default()).await?;
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

macro_rules! make_migration_func {
	($name:ident, $state:ident, $func:block) => {
		MigrationClosure(Arc::new(Mutex::new(Box::new(
			|mut state: MigrationState| {
				let $state = state.clone();
				Box::pin(async move { $func })
			},
		))))
	};
}

macro_rules! build_migration_set {
    ($state:ident, $(($name:ident, $func:block)),*) => {{
      let mut v = Vec::new();
      $(
      {
        let func = make_migration_func!($name, $state, $func);
        v.push(func);
      }
      )*
      v
    }}
  }

mod tests {
	use super::*;

	async fn migration_success() {
		let state: MigrationState = Default::default();
		build_migration_set!(state, (foo, { Ok(state) }), (bar, { Ok(state) }));
	}
}
