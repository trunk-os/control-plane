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
		"successful_run_with_successful_check" => Some(Migration {
			name: name.to_string(),
			dependencies: Default::default(),
			check: Some(Box::new(|| Ok(()))),
			run: Box::new(|| Ok(())),
			post_check: None,
		}),
		"successful_run_with_successful_post_check" => Some(Migration {
			name: name.to_string(),
			dependencies: Default::default(),
			check: None,
			run: Box::new(|| Ok(())),
			post_check: Some(Box::new(|| Ok(()))),
		}),
		"successful_run_with_successful_both_checks" => Some(Migration {
			name: name.to_string(),
			dependencies: Default::default(),
			check: Some(Box::new(|| Ok(()))),
			run: Box::new(|| Ok(())),
			post_check: Some(Box::new(|| Ok(()))),
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

	#[tokio::test]
	async fn run_with_checks() {
		assert!(
			execute_migration("successful_run_with_successful_check")
				.await
				.is_ok()
		);
		assert!(
			execute_migration("successful_run_with_successful_post_check")
				.await
				.is_ok()
		);
		assert!(
			execute_migration("successful_run_with_successful_both_checks")
				.await
				.is_ok()
		);
	}

	#[tokio::test]
	#[ignore]
	async fn run_with_dependencies() {}

	#[tokio::test]
	#[ignore]
	async fn run_with_recovery() {}
}

mod migrator {}
