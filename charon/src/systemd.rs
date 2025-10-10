use crate::{CompiledPackage, DEFAULT_CHARON_BIN_PATH};
use anyhow::{Result, anyhow};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const SYSTEMD_SERVICE_ROOT: &str = "/etc/systemd/system";

const UNIT_TEMPLATE: &str = r#"
[Unit]
Description=Charon launcher for @PACKAGE_NAME@, version @PACKAGE_VERSION@

[Service]
ExecStart=@CHARON_PATH@ -b @BUCKLE_SOCKET@ -r @REGISTRY_PATH@ launch @PACKAGE_NAME@ @PACKAGE_VERSION@ @VOLUME_ROOT@
ExecStop=@CHARON_PATH@ -b @BUCKLE_SOCKET@ -r @REGISTRY_PATH@ stop @PACKAGE_NAME@ @PACKAGE_VERSION@ @VOLUME_ROOT@
Restart=always
TimeoutSec=300

[Install]
Alias=@PACKAGE_FILENAME@.service
"#;

#[derive(Debug, Clone)]
pub struct SystemdUnit {
	buckle_socket: PathBuf,
	package: CompiledPackage,
	systemd_root: Option<PathBuf>,
	charon_path: Option<PathBuf>,
}

impl SystemdUnit {
	pub fn new(
		buckle_socket: PathBuf, package: CompiledPackage, systemd_root: Option<PathBuf>,
		charon_path: Option<PathBuf>,
	) -> Self {
		Self {
			buckle_socket,
			package,
			systemd_root,
			charon_path,
		}
	}

	pub fn buckle(&self) -> Result<buckle::client::Client> {
		Ok(buckle::client::Client::new(self.buckle_socket.clone())?)
	}

	pub fn service_name(&self) -> String {
		format!("{}.service", self.package.title)
	}

	pub fn filename(&self) -> PathBuf {
		format!(
			"{}/{}.service",
			self.systemd_root
				.clone()
				.unwrap_or(SYSTEMD_SERVICE_ROOT.into())
				.display(),
			self.package.title
		)
		.into()
	}

	pub async fn unit(&self, registry_path: &Path, volume_root: &Path) -> Result<String> {
		let mut out = String::new();
		let mut variable = String::new();
		let mut in_variable = false;

		for ch in UNIT_TEMPLATE.chars() {
			if ch == '@' {
				in_variable = if in_variable {
					match variable.as_str() {
						"PACKAGE_NAME" => out.push_str(&self.package.title.name),
						"PACKAGE_VERSION" => out.push_str(&self.package.title.version),
						"PACKAGE_FILENAME" => out.push_str(&self.package.title.to_string()),
						"REGISTRY_PATH" => out.push_str(&registry_path.to_string_lossy()),
						"BUCKLE_SOCKET" => out.push_str(&self.buckle_socket.to_string_lossy()),
						"VOLUME_ROOT" => out.push_str(&volume_root.to_string_lossy()),
						"CHARON_PATH" => {
							out.push_str(
								self.charon_path
									.clone()
									.unwrap_or(DEFAULT_CHARON_BIN_PATH.into())
									.to_str()
									.unwrap(),
							);
						}
						_ => {
							return Err(anyhow!("invalid template variable '{}'", variable));
						}
					};
					variable = String::new();

					false
				} else {
					true
				}
			} else if in_variable {
				variable.push(ch)
			} else {
				out.push(ch)
			}
		}

		Ok(out)
	}

	pub async fn create_unit(&self, registry_path: &Path, volume_root: &Path) -> Result<()> {
		let mut f = std::fs::OpenOptions::new()
			.create(true)
			.truncate(true)
			.write(true)
			.open(self.filename())
			.map_err(|e| {
				anyhow!(
					"Could not create service unit {}: {}",
					self.filename().display(),
					e
				)
			})?;
		f.write_all(
			self.unit(registry_path, volume_root)
				.await
				.map_err(|e| {
					anyhow!(
						"Could not generate service unit {}: {}",
						self.filename().display(),
						e
					)
				})?
				.as_bytes(),
		)
		.map_err(|e| {
			anyhow!(
				"Could not write service unit {}: {}",
				self.filename().display(),
				e
			)
		})?;

		let buckle = self.buckle()?;

		buckle.systemd().await?.reload().await?;
		buckle
			.systemd()
			.await?
			.start_unit(format!("{}.service", self.package.title))
			.await?;

		Ok(())
	}

	pub async fn remove_unit(&self) -> Result<()> {
		// FIXME: this should not be here! use GRPC!
		let buckle = self.buckle()?;
		buckle
			.systemd()
			.await?
			.stop_unit(format!("{}.service", self.package.title))
			.await?;
		std::fs::remove_file(self.filename()).map_err(|e| {
			anyhow!(
				"Could not remove service unit {}: {}",
				self.filename().display(),
				e
			)
		})?;

		buckle.systemd().await?.reload().await?;

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use std::path::PathBuf;

	use super::SystemdUnit;
	use crate::server::tests::start_server;
	use crate::{CompiledPackage, Registry, SYSTEMD_SERVICE_ROOT};
	use anyhow::Result;

	async fn load(registry: &Registry, name: &str, version: &str) -> Result<CompiledPackage> {
		registry.load(name, version)?.compile().await
	}

	#[tokio::test]
	async fn unit_names() {
		let (config, _, _, buckle_info) =
			start_server(false, Some("charon-test-unit-names".into())).await;
		let registry = Registry::new("testdata/registry".into());
		let unit = SystemdUnit::new(
			config.buckle_socket,
			load(&registry, "podman-test", "0.0.2").await.unwrap(),
			Some(SYSTEMD_SERVICE_ROOT.into()),
			Some("/usr/bin/charon".into()),
		);
		assert_eq!(
			unit.filename().as_os_str(),
			"/etc/systemd/system/podman-test-0.0.2.service"
		);

		assert_eq!(unit.service_name(), "podman-test-0.0.2.service");
		if let Some(buckle_info) = buckle_info {
			let _ = buckle::testutil::destroy_zpool("charon-test-unit-names", Some(&buckle_info.2));
		}
	}

	#[tokio::test]
	async fn unit_contents() {
		let (config, _, _, buckle_info) =
			start_server(false, Some("charon-test-unit-contents".into())).await;
		let registry = Registry::new("testdata/registry".into());
		let pkg = load(&registry, "podman-test", "0.0.2").await.unwrap();
		let unit = SystemdUnit::new(
			config.buckle_socket.clone(),
			pkg,
			Some(crate::SYSTEMD_SERVICE_ROOT.into()),
			Some(crate::DEFAULT_CHARON_BIN_PATH.into()),
		);
		let text = unit
			.unit(&registry.path(), &PathBuf::from("/tmp/volroot"))
			.await
			.unwrap();
		assert_eq!(
			text,
			r#"
[Unit]
Description=Charon launcher for podman-test, version 0.0.2

[Service]
ExecStart=/usr/bin/charon -b @BUCKLE_SOCKET@ -r testdata/registry launch podman-test 0.0.2 /tmp/volroot
ExecStop=/usr/bin/charon -b @BUCKLE_SOCKET@ -r testdata/registry stop podman-test 0.0.2 /tmp/volroot
Restart=always
TimeoutSec=300

[Install]
Alias=podman-test-0.0.2.service
"#.replace("@BUCKLE_SOCKET@", &config.buckle_socket.to_string_lossy().to_string()),
		);
		if let Some(buckle_info) = buckle_info {
			let _ =
				buckle::testutil::destroy_zpool("charon-test-unit-contents", Some(&buckle_info.2));
		}
	}
}
