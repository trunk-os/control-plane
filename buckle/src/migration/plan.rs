use super::MigrationPlan;
use anyhow::Result;
use std::path::PathBuf;
use vayama::{Migration, Migrator, migration_func, utils::*};

fn migrations() -> Vec<Migration> {
	vec![
		Migration::new_with_check(
			"ensure node_exporter".into(),
			migration_func!({ Ok(()) }),
			migration_func!({ Ok(()) }),
		),
		Migration::new_with_check(
			"ensure prometheus".into(),
			migration_func!({ Ok(()) }),
			migration_func!({ Ok(()) }),
		),
		Migration::new_with_check(
			"ensure grafana".into(),
			migration_func!({ Ok(()) }),
			migration_func!({ Ok(()) }),
		),
		Migration {
			name: "link grafana and prometheus".into(),
			run: migration_func!({ Ok(()) }),
			dependencies: vec!["ensure prometheus".into(), "ensure grafana".into()],
			check: None,
			post_check: Some(migration_func!({ Ok(()) })),
		},
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
