use crate::*;
use std::sync::atomic::{AtomicBool, Ordering};

static STATE: AtomicBool = AtomicBool::new(false);

fn get_state() -> bool {
	STATE.load(Ordering::Acquire)
}

fn successful_run_func() -> MigrationFunc {
	Box::new(|| {
		STATE.store(true, Ordering::SeqCst);
		Ok(())
	})
}

fn error_run_func() -> MigrationFunc {
	Box::new(|| Err(MigrationError::Unknown))
}

fn successful_check() -> Option<MigrationFunc> {
	Some(Box::new(|| Ok(())))
}

fn error_check() -> Option<MigrationFunc> {
	Some(Box::new(|| Err(MigrationError::Unknown)))
}

// this is less effort than populating a Map etc
fn get_migration(name: &str) -> Option<Migration> {
	match name {
		"successful_run" => Some(Migration {
			name: name.to_string(),
			dependencies: Default::default(),
			check: None,
			run: successful_run_func(),
			post_check: None,
		}),
		"run_only_with_error" => Some(Migration {
			name: name.to_string(),
			dependencies: Default::default(),
			check: None,
			run: error_run_func(),
			post_check: None,
		}),
		"successful_run_with_successful_check" => Some(Migration {
			name: name.to_string(),
			dependencies: Default::default(),
			check: successful_check(),
			run: successful_run_func(),
			post_check: None,
		}),
		"successful_run_with_successful_post_check" => Some(Migration {
			name: name.to_string(),
			dependencies: Default::default(),
			check: None,
			run: successful_run_func(),
			post_check: successful_check(),
		}),
		"successful_run_with_successful_both_checks" => Some(Migration {
			name: name.to_string(),
			dependencies: Default::default(),
			check: successful_check(),
			run: successful_run_func(),
			post_check: successful_check(),
		}),
		"successful_run_with_failing_check" => Some(Migration {
			name: name.to_string(),
			dependencies: Default::default(),
			check: error_check(),
			run: successful_run_func(),
			post_check: None,
		}),
		"successful_run_with_failing_post_check" => Some(Migration {
			name: name.to_string(),
			dependencies: Default::default(),
			check: None,
			run: successful_run_func(),
			post_check: error_check(),
		}),
		"successful_run_with_failing_both_checks" => Some(Migration {
			name: name.to_string(),
			dependencies: Default::default(),
			check: error_check(),
			run: successful_run_func(),
			post_check: error_check(),
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
	STATE.store(false, Ordering::SeqCst);

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
		assert!(get_state());
		assert!(execute_migration("run_only_with_error").await.is_err());
		assert!(!get_state());
	}

	#[tokio::test]
	async fn run_with_checks() {
		for name in vec![
			"successful_run_with_successful_check",
			"successful_run_with_successful_post_check",
			"successful_run_with_successful_both_checks",
		] {
			assert!(execute_migration(name).await.is_ok());
			assert!(get_state(), "{}", name);
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
			assert!(!get_state(), "{}", name);
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
