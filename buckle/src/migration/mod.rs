#![allow(dead_code)]
use std::{collections::HashMap, pin::Pin};
use thiserror::Error;

mod plans;

pub type MigrationState = HashMap<String, String>;
pub type MigrationResult = Result<MigrationState, MigrationError>;
pub type MigrationFunc =
	Box<dyn Fn(MigrationState) -> Pin<Box<dyn Future<Output = MigrationResult>>>>;

#[derive(Debug, Clone, Default, Error)]
pub enum MigrationError {
	#[default]
	#[error("Unknown Error")]
	Unknown,
	#[error("Unknown Error: {0}")]
	UnknownWithMessage(String),
}

pub struct Migration(Vec<MigrationFunc>);

impl Migration {
	pub async fn run(&self, mut state: MigrationState) -> Result<(), MigrationError> {
		for func in &self.0 {
			state = (func)(state).await?;
		}

		Ok(())
	}
}

pub async fn run_migrations(migrations: Vec<Migration>) -> anyhow::Result<()> {
	for migration in &migrations {
		migration.run(Default::default()).await?;
	}

	Ok(())
}

mod tests {
	#![allow(unused_variables)]
	use super::*;

	macro_rules! make_migration_func {
		($name:ident, $state:ident, $func:block) => {
			fn $name(
				state: &mut MigrationState,
			) -> Pin<Box<impl Future<Output = MigrationResult>>> {
				let $state = state.clone();
				Box::pin(async move { ($func) })
			}
		};
	}

	macro_rules! build_migration_set {
    ($(($name:ident, $state:ident, $func:block)),*) => {{
    $(
    make_migration_func!($name, $state, $func);
    )*
    }}
  }

	async fn migration_test() {
		let state: MigrationState = Default::default();
		build_migration_set!((fart, state, { Ok(state) }));
	}
}
