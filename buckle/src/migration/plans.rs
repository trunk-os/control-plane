use super::utils::*;
use super::*;
use crate::systemd_unit;
use crate::{build_migration_set, make_migration_func};
use std::time::Duration;

#[macro_export]
#[rustfmt::skip]
macro_rules! build_container_migration {
	($name:ident, $description:expr, $command:expr) => {{
		use super::utils::*;
		use crate::systemd_unit;
		use std::time::Duration;

		let state = MigrationState::default();
		build_migration_set!(state, {
			match zfs(vec!["list", "trunk/$name"]).await {
				Ok(_) => {}
				Err(_) => {
					zfs(vec!["create", "trunk/$name", "-o", "quota=50G"]).await?;
				}
			}

      let unit = systemd_unit!(
        "trunk-$name",
        ("Unit", (("Description" => &format!("Trunk: {}", $description)),)),
        ("Service", (
          ("ExecStart" => $command),
          ("ExecStop" => "podman stop trunk-$name"),
          ("Restart" => "always"),
          ("TimeoutSec" => "300"),
        )),
        ("Install", (
          ("Alias" => "trunk-$name.service"),
          ("WantedBy" => "network-online.target"),
        )),
      );

			unit.write(None)?;

			systemctl(vec!["daemon-reload"]).await?;
			tokio::time::sleep(Duration::from_millis(200)).await;
			systemctl(vec!["restart", "trunk-$name.service"]).await?;

			Ok(state)
		})
	}};
}

fn prometheus() -> Migration {
	build_container_migration!(
		prometheus,
		"Prometheus Query Service",
		"podman run -u 0 --net host -d -v /trunk/prometheus:/prometheus:shared --name trunk-prometheus quay.io/trunk-os/prometheus"
	)
}

#[rustfmt::skip]
fn grafana() -> Migration {
	let state = MigrationState::default();
	build_migration_set!(state, {
		match zfs(vec!["list", "trunk/grafana"]).await {
			Ok(_) => {}
			Err(_) => {
				zfs(vec!["create", "trunk/grafana", "-o", "quota=50G"]).await?;
			}
		}

		let unit = systemd_unit!(
		  "trunk-grafana",
		  ("Unit", (("Description" => "Trunk: Grafana Dashboard Service"),)),
		  ("Service", (
        ("ExecStart" => "podman run -it -u 0 --net host -d --name trunk-grafana -v /trunk/grafana:/var/lib/grafana:shared,rw quay.io/trunk-os/grafana"),
        ("ExecStop" => "podman stop trunk-grafana"),
        ("Restart" => "always"),
        ("TimeoutSec" => "300"),
		  )),
		  ("Install", (
        ("Alias" => "trunk-grafana.service"),
        ("WantedBy" => "network-online.target"),
		  )),
		);

		unit.write(None)?;

		systemctl(vec!["daemon-reload"]).await?;
		tokio::time::sleep(Duration::from_millis(200)).await;
		systemctl(vec!["restart", "trunk-grafana.service"]).await?;

		Ok(state)
	})
}

fn node_exporter() -> Migration {
	build_container_migration!(
		node_exporter,
		"node-exporter Metrics Service",
		"podman run -it -d --cap-add SYS_TIME --name trunk-node-exporter --net host --pid host -v /:/host:ro,rslave quay.io/trunk-os/node-exporter --path.rootfs=/host"
	)
}
