use crate::*;
use std::{
	collections::BTreeMap,
	sync::{Arc, LazyLock, Mutex},
};

static STATE: LazyLock<Arc<Mutex<BTreeMap<String, bool>>>> = LazyLock::new(|| Default::default());

fn get_state(name: &str) -> bool {
	*STATE.lock().unwrap().get(name).unwrap_or(&false)
}

fn successful_run_func(name: &'static str) -> MigrationFunc {
	Box::new(move || {
		STATE.lock().unwrap().insert(name.to_string(), true);
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
fn get_migration(name: &'static str) -> Option<Migration> {
	match name {
		"successful_run" => Some(Migration {
			name: name.to_string(),
			dependencies: Default::default(),
			check: None,
			run: successful_run_func(name),
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
			run: successful_run_func(name),
			post_check: None,
		}),
		"successful_run_with_successful_post_check" => Some(Migration {
			name: name.to_string(),
			dependencies: Default::default(),
			check: None,
			run: successful_run_func(name),
			post_check: successful_check(),
		}),
		"successful_run_with_successful_both_checks" => Some(Migration {
			name: name.to_string(),
			dependencies: Default::default(),
			check: successful_check(),
			run: successful_run_func(name),
			post_check: successful_check(),
		}),
		"successful_run_with_failing_check" => Some(Migration {
			name: name.to_string(),
			dependencies: Default::default(),
			check: error_check(),
			run: successful_run_func(name),
			post_check: None,
		}),
		"successful_run_with_failing_post_check" => Some(Migration {
			name: name.to_string(),
			dependencies: Default::default(),
			check: None,
			run: successful_run_func(name),
			post_check: error_check(),
		}),
		"successful_run_with_failing_both_checks" => Some(Migration {
			name: name.to_string(),
			dependencies: Default::default(),
			check: error_check(),
			run: successful_run_func(name),
			post_check: error_check(),
		}),
		"successful_run_with_dependencies" => Some(Migration {
			name: name.to_string(),
			dependencies: vec!["dependency".into()],
			check: None,
			run: successful_run_func(name),
			post_check: None,
		}),
		"successful_run_based_on_state" => Some(Migration {
			name: name.to_string(),
			dependencies: Default::default(),
			check: None,
			run: Box::new(|| {
				if STATE.lock().unwrap().contains_key(name) {
					Ok(())
				} else {
					STATE.lock().unwrap().insert(name.to_string(), true);
					Err(MigrationError::Unknown)
				}
			}),
			post_check: None,
		}),
		_ => None,
	}
}

async fn execute_migration(name: &'static str) -> Result<(), MigrationError> {
	let state: MigrationState = Default::default();
	execute_migration_with_state(name, state).await
}

async fn execute_migration_with_state(
	name: &'static str, state: MigrationState,
) -> Result<(), MigrationError> {
	get_migration(name)
		.expect(&format!("test migration ({}) missing from table", name))
		.execute(&state)
		.await
}

mod migration {
	use super::*;
	use std::collections::BTreeSet;

	#[tokio::test]
	async fn basic_run() {
		assert!(execute_migration("successful_run").await.is_ok());
		assert!(get_state("successful_run"));
		assert!(execute_migration("run_only_with_error").await.is_err());
		assert!(!get_state("run_only_with_error"));
	}

	#[tokio::test]
	async fn run_with_checks() {
		for name in vec![
			"successful_run_with_successful_check",
			"successful_run_with_successful_post_check",
			"successful_run_with_successful_both_checks",
		] {
			assert!(execute_migration(name).await.is_ok());
			assert!(get_state(name), "{}", name);
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
			assert!(!get_state(name), "{}", name);
		}
	}

	#[tokio::test]
	async fn run_with_dependencies() {
		let mut state = MigrationState::default();
		state.failed_migrations = BTreeSet::new();
		state.failed_migrations.insert("dependency".into());
		assert!(
			execute_migration_with_state("successful_run_with_dependencies", state.clone())
				.await
				.is_err()
		);
		assert!(!get_state("successful_run_with_dependencies"));

		state.failed_migrations = Default::default();
		assert!(
			execute_migration_with_state("successful_run_with_dependencies", state)
				.await
				.is_ok()
		);
		assert!(get_state("successful_run_with_dependencies"))
	}
}

mod migrator {
	use std::collections::BTreeSet;

	use anyhow::Result;
	use tempfile::TempDir;

	use super::*;

	fn create_migrator(migrations: Vec<Migration>) -> Result<(Migrator, TempDir)> {
		STATE.lock().unwrap().clear();
		let dir = tempfile::tempdir()?;
		Ok((
			Migrator::new_with_root(migrations, dir.path().to_path_buf())?,
			dir,
		))
	}

	#[tokio::test]
	async fn clean_run() {
		let (mut migrator, dir) = create_migrator(vec![
			get_migration("successful_run").unwrap(),
			get_migration("successful_run_with_successful_check").unwrap(),
			get_migration("successful_run_with_successful_post_check").unwrap(),
			get_migration("successful_run_with_successful_both_checks").unwrap(),
		])
		.unwrap();

		let mut i = 0;

		while let Ok(res) = migrator.execute().await {
			assert_eq!(res.unwrap(), i);
			i += 1;
		}

		let mut f = std::fs::OpenOptions::new()
			.read(true)
			.open(dir.path().join(MIGRATION_FILENAME))
			.unwrap();
		let state: MigrationState = serde_json::from_reader(&mut f).unwrap();
		assert_eq!(migrator.state, state);

		assert!(!migrator.more_migrations());
	}

	#[tokio::test]
	async fn run_with_failures() {
		let (mut migrator, dir) = create_migrator(vec![
			get_migration("run_only_with_error").unwrap(),
			get_migration("successful_run").unwrap(),
			get_migration("successful_run_with_successful_check").unwrap(),
		])
		.unwrap();

		let mut i = 0;

		while let Ok(res) = migrator.execute().await {
			if i == 0 {
				assert_eq!(res, None);
			} else {
				assert_eq!(res, Some(i), "{}", i);
			}

			i += 1;
		}

		let mut f = std::fs::OpenOptions::new()
			.read(true)
			.open(dir.path().join(MIGRATION_FILENAME))
			.unwrap();
		let state: MigrationState = serde_json::from_reader(&mut f).unwrap();
		assert_eq!(migrator.state, state);

		let mut res = BTreeSet::new();
		res.insert("run_only_with_error".into());
		assert_eq!(migrator.state.failed_migrations, res);

		assert!(!migrator.more_migrations());
	}

	#[tokio::test]
	async fn run_twice_with_new_migrations() {
		let (mut migrator, dir) = create_migrator(vec![
			get_migration("successful_run_based_on_state").unwrap(),
			get_migration("successful_run").unwrap(),
			get_migration("successful_run_with_successful_check").unwrap(),
		])
		.unwrap();

		let mut i = 0;

		while let Ok(res) = migrator.execute().await {
			if i == 0 {
				assert_eq!(res, None);
			} else {
				assert_eq!(res, Some(i), "{}", i);
			}

			i += 1;
		}

		let mut f = std::fs::OpenOptions::new()
			.read(true)
			.open(dir.path().join(MIGRATION_FILENAME))
			.unwrap();
		let state: MigrationState = serde_json::from_reader(&mut f).unwrap();
		assert_eq!(migrator.state, state);

		let mut res = BTreeSet::new();
		res.insert("successful_run_based_on_state".into());
		assert_eq!(migrator.state.failed_migrations, res);

		assert!(!migrator.more_migrations());

		assert!(migrator.execute_failed().await.is_ok());

		let mut f = std::fs::OpenOptions::new()
			.read(true)
			.open(dir.path().join(MIGRATION_FILENAME))
			.unwrap();
		let state: MigrationState = serde_json::from_reader(&mut f).unwrap();
		assert_eq!(migrator.state, state);
		assert_eq!(migrator.state.failed_migrations, Default::default());
	}

	#[tokio::test]
	#[ignore]
	async fn run_with_failing_checks() {}

	#[tokio::test]
	#[ignore]
	async fn run_with_failing_dependencies() {}
}
