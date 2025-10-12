use crate::*;

// this is less effort than populating a Map etc
fn get_migration(name: &str) -> Option<Migration> {
	match name {
		"successful_run" => Some(Migration {
			name: name.to_string(),
			dependencies: Default::default(),
			check: None,
			run: Box::new(|| Ok(())),
			post_check: None,
		}),
		"run_only_with_error" => Some(Migration {
			name: name.to_string(),
			dependencies: Default::default(),
			check: None,
			run: Box::new(|| Err(MigrationError::Unknown)),
			post_check: None,
		}),
		_ => None,
	}
}

async fn execute_migration(name: &str) -> Result<(), MigrationError> {
	let state: MigrationState = Default::default();
	execute_migration_with_state(name, state).await
}

async fn execute_migration_with_state(
	name: &str, state: MigrationState,
) -> Result<(), MigrationError> {
	get_migration(name)
		.expect(&format!("test migration ({}) missing from table", name))
		.execute(&state)
		.await
}

mod migration {
	use super::*;

	#[tokio::test]
	async fn basic_run() {
		assert!(execute_migration("successful_run").await.is_ok());
		assert!(execute_migration("run_only_with_error").await.is_err());
	}

	#[test]
	#[ignore]
	fn run_with_checks() {}

	#[test]
	#[ignore]
	fn run_with_dependencies() {}

	#[test]
	#[ignore]
	fn run_with_recovery() {}
}

mod migrator {}
