use super::MigrationPlan;
use anyhow::Result;
use std::{path::PathBuf, sync::LazyLock};
use vayama::{Migration, Migrator, migration_func};

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
	let migrator = Migrator::new_with_root(migrations(), root.clone())?;

	Ok(MigrationPlan::new(migrator, root, zpool))
}
