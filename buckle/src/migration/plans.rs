use super::*;
use crate::{build_migration_set, make_migration_func};
use std::{collections::HashMap, time::Duration};

// NOTE: if they're not in this list, they basically don't exist
pub fn migrations() -> HashMap<&'static str, Migration> {
	HashMap::from([
		("node-exporter", node_exporter()),
		("prometheus", prometheus()),
		("grafana", grafana()),
	])
}

pub async fn boot_service(name: &str) -> anyhow::Result<()> {
	let client = crate::systemd::Systemd::new_system().await?;
	client.reload().await?;
	tokio::time::sleep(Duration::from_millis(200)).await;
	client.stop(name.to_string()).await?;
	client.start(name.to_string()).await?;
	client.enable(name.to_string()).await?;
	Ok(())
}

#[macro_export]
#[rustfmt::skip]
macro_rules! build_container_migration {
	($name:expr, $description:expr, $command:expr) => {{
		use super::utils::*;
		use crate::systemd_unit;

		let state = MigrationState::default();
		build_migration_set!(state, {
      let volname = format!("trunk/{}", $name);

			match zfs(vec!["list", &volname]).await {
				Ok(_) => {}
				Err(_) => {
					zfs(vec!["create", &volname, "-o", "quota=50G"]).await?;
				}
			}

      let unit = systemd_unit!(
        &format!("trunk-{}", $name),
        ("Unit", (("Description" => &format!("Trunk: {}", $description)),)),
        ("Service", (
          ("ExecStart" => $command),
          ("ExecStop" => &format!("podman rm -f trunk-{}", $name)),
          ("Restart" => "always"),
          ("TimeoutSec" => "300"),
        )),
        ("Install", (
          ("Alias" => &format!("trunk-{}.service", $name)),
          ("WantedBy" => "network-online.target"),
        )),
      );

			unit.write(None)?;
      boot_service(&format!("trunk-{}.service", $name)).await?;

			Ok(state)
		})
	}};
}

fn prometheus() -> Migration {
	build_container_migration!(
		"prometheus",
		"Prometheus Query Service",
		"podman run -u 0 --net host -it -v /trunk/prometheus:/prometheus:shared --name trunk-prometheus quay.io/trunk-os/prometheus"
	)
}

fn grafana() -> Migration {
	build_container_migration!(
		"grafana",
		"Grafana Dashboard Service",
		"podman run -it -u 0 --net host -it --name trunk-grafana -v /trunk/grafana:/var/lib/grafana:shared,rw quay.io/trunk-os/grafana"
	)
}

fn node_exporter() -> Migration {
	build_container_migration!(
		"node-exporter",
		"node-exporter Metrics Service",
		"podman run -it --cap-add SYS_TIME --name trunk-node-exporter --net host --pid host -v /:/host:ro,rslave quay.io/trunk-os/node-exporter --path.rootfs=/host"
	)
}
