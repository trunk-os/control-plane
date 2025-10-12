use crate::*;
use std::sync::atomic::{AtomicBool, Ordering};

static STATE: AtomicBool = AtomicBool::new(false);

// this is less effort than populating a Map etc
fn get_migration(name: &str) -> Option<Migration> {
	match name {
		"successful_run" => Some(Migration {
			name: name.to_string(),
			dependencies: Default::default(),
			check: None,
			run: Box::new(|| {
				STATE.store(true, Ordering::Release);
				Ok(())
			}),
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
			run: Box::new(|| {
				STATE.store(true, Ordering::Release);
				Ok(())
			}),
			post_check: None,
		}),
		"successful_run_with_successful_post_check" => Some(Migration {
			name: name.to_string(),
			dependencies: Default::default(),
			check: None,
			run: Box::new(|| {
				STATE.store(true, Ordering::Release);
				Ok(())
			}),
			post_check: Some(Box::new(|| Ok(()))),
		}),
		"successful_run_with_successful_both_checks" => Some(Migration {
			name: name.to_string(),
			dependencies: Default::default(),
			check: Some(Box::new(|| Ok(()))),
			run: Box::new(|| {
				STATE.store(true, Ordering::Release);
				Ok(())
			}),
			post_check: Some(Box::new(|| Ok(()))),
		}),
		"successful_run_with_failing_check" => Some(Migration {
			name: name.to_string(),
			dependencies: Default::default(),
			check: Some(Box::new(|| Err(MigrationError::Unknown))),
			run: Box::new(|| {
				STATE.store(true, Ordering::Release);
				Ok(())
			}),
			post_check: None,
		}),
		"successful_run_with_failing_post_check" => Some(Migration {
			name: name.to_string(),
			dependencies: Default::default(),
			check: None,
			run: Box::new(|| {
				STATE.store(true, Ordering::Release);
				Ok(())
			}),
			post_check: Some(Box::new(|| Err(MigrationError::Unknown))),
		}),
		"successful_run_with_failing_both_checks" => Some(Migration {
			name: name.to_string(),
			dependencies: Default::default(),
			check: Some(Box::new(|| Err(MigrationError::Unknown))),
			run: Box::new(|| {
				STATE.store(true, Ordering::Release);
				Ok(())
			}),
			post_check: Some(Box::new(|| Err(MigrationError::Unknown))),
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
	STATE.store(false, Ordering::Release);

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
		assert!(STATE.load(Ordering::Acquire));
		assert!(execute_migration("run_only_with_error").await.is_err());
		assert!(!STATE.load(Ordering::Acquire));
	}

	#[tokio::test]
	async fn run_with_checks() {
		for name in vec![
			"successful_run_with_successful_check",
			"successful_run_with_successful_post_check",
			"successful_run_with_successful_both_checks",
		] {
			assert!(execute_migration(name).await.is_ok());
			assert!(STATE.load(Ordering::Acquire), "{}", name);
		}

		// state is different here for post_check (run succeeded, post did not); that state check is
		// below this one
		assert!(
			execute_migration("successful_run_with_failing_post_check")
				.await
				.is_err()
		);

		for name in vec![
			"successful_run_with_failing_check",
			"successful_run_with_failing_both_checks",
		] {
			assert!(execute_migration(name).await.is_err());
			assert!(!STATE.load(Ordering::Acquire), "{}", name);
		}
	}

	#[tokio::test]
	#[ignore]
	async fn run_with_dependencies() {}

	#[tokio::test]
	#[ignore]
	async fn run_with_recovery() {}
}

#[allow(unused)]
mod migrator {
	use super::*;
}
