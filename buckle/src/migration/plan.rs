use super::MigrationPlan;
use anyhow::Result;
use std::path::PathBuf;
use vayama::{Migration, Migrator, migration_func, utils::*};

#[macro_export]
macro_rules! build_migration {
	($name:expr, $run:block, $($key:ident => $value:block),*) => {
		Migration {
			name: $name.into(),
			run: migration_func!($run),
      dependencies: Default::default(),
      $(
			$key: Some(migration_func!($value)),
      )*
      ..Default::default()
		}
	};
	($name:expr, $run:block, dependencies => $deps:expr, $($key:ident => $value:block),*) => {
		Migration {
			name: $name.into(),
			run: migration_func!($run),
      dependencies: $deps,
      $(
			$key: Some(migration_func!($value)),
      )*
      ..Default::default()
		}
	};
}

fn migrations() -> Vec<Migration> {
	vec![
		build_migration!("ensure node_exporter",
			{ Ok(()) },
			check => { Ok(()) }
		),
		build_migration!("ensure prometheus",
			{ Ok(()) },
			check => { Ok(()) }
		),
		build_migration!("ensure grafana",
			{ Ok(()) },
			check => { Ok(()) }
		),
		build_migration!("link grafana and prometheus",
			{ Ok(()) },
			dependencies => vec![
				"ensure_prometheus".into(),
				"ensure grafana".into(),
			],
			post_check => { Ok(()) }
		),
	]
}

pub(crate) async fn load_migrations(
	root: Option<PathBuf>, zpool: Option<String>,
) -> Result<MigrationPlan> {
	let migrator = Migrator::new_with_root(migrations(), Default::default(), root.clone())?;

	Ok(MigrationPlan::new(migrator, root, zpool))
}

// command status is just output on success; so shape these predicate commands so that if output is empty, the
// command was successful but the predicate is false.

async fn container_is_running(name: &str) -> Result<bool> {
	Ok(podman(vec!["ps", "-f", &format!("name={}", name), "-q"])
		.await?
		.is_empty())
}

async fn zfs_exists(name: &str) -> Result<bool> {
	Ok(zfs(vec!["list", name]).await?.is_empty())
}
