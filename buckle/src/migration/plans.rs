use super::utils::*;
use super::*;
use crate::{build_migration_set, make_migration_func};

fn prometheus() -> Migration {
	let state = MigrationState::default();
	build_migration_set!(
		state,
		(check_prometheus, {
			podman(vec!["pull", "quay.io/prometheus/prometheus"]).await?;
			Ok(state)
		}),
		(install_prometheus, { Ok(state) }),
		(configure_prometheus, { Ok(state) }),
		(restart_prometheus, { Ok(state) })
	)
}

fn grafana() -> Migration {
	let state = MigrationState::default();
	build_migration_set!(
		state,
		(check_grafana, { Ok(state) }),
		(install_grafana, { Ok(state) }),
		(configure_grafana, { Ok(state) }),
		(restart_grafana, { Ok(state) })
	)
}

fn node_exporter() -> Migration {
	let state = MigrationState::default();
	build_migration_set!(
		state,
		(check_node_exporter, { Ok(state) }),
		(install_node_exporter, { Ok(state) }),
		(configure_node_exporter, { Ok(state) }),
		(restart_node_exporter, { Ok(state) })
	)
}
