use crate::{CompiledPackage, DEFAULT_CHARON_BIN_PATH};
use anyhow::{Result, anyhow};
use std::io::Write;
use std::path::PathBuf;

pub const SYSTEMD_SERVICE_ROOT: &str = "/etc/systemd/system";

const UNIT_TEMPLATE: &str = r#"
[Unit]
Description=Charon launcher for @PACKAGE_NAME@, version @PACKAGE_VERSION@

[Service]
ExecStart=@CHARON_PATH@ -r @REGISTRY_PATH@ launch @PACKAGE_NAME@ @PACKAGE_VERSION@ @VOLUME_ROOT@
ExecStop=@CHARON_PATH@ -r @REGISTRY_PATH@ stop @PACKAGE_NAME@ @PACKAGE_VERSION@ @VOLUME_ROOT@
Restart=always
TimeoutSec=300

[Install]
Alias=@PACKAGE_FILENAME@.service
"#;

#[derive(Debug, Clone)]
pub struct SystemdUnit {
	package: CompiledPackage,
	systemd_root: Option<PathBuf>,
	charon_path: Option<PathBuf>,
}

impl SystemdUnit {
	pub fn new(
		package: CompiledPackage, systemd_root: Option<PathBuf>,
		charon_path: Option<PathBuf>,
	) -> Self {
		Self {
			package,
			systemd_root,
			charon_path,
		}
	}

	pub fn service_name(&self) -> String {
		format!("{}.service", self.package.title).into()
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

	pub fn unit(
		&self, registry_path: PathBuf, volume_root: PathBuf,
	) -> Result<String> {
		let mut out = String::new();
		let mut variable = String::new();
		let mut in_variable = false;

		for ch in UNIT_TEMPLATE.chars() {
			if ch == '@' {
				in_variable = if in_variable {
					match variable.as_str() {
						"PACKAGE_NAME" => {
							out.push_str(&self.package.title.name)
						}
						"PACKAGE_VERSION" => {
							out.push_str(&self.package.title.version)
						}
						"PACKAGE_FILENAME" => out
							.push_str(&self.package.title.to_string()),
						"VOLUME_ROOT" => out.push_str(
							volume_root.to_str().unwrap_or_default(),
						),
						"REGISTRY_PATH" => out.push_str(
							registry_path.to_str().unwrap_or_default(),
						),
						"CHARON_PATH" => {
							out.push_str(
								self.charon_path
									.clone()
									.unwrap_or(
										DEFAULT_CHARON_BIN_PATH.into(),
									)
									.to_str()
									.unwrap(),
							);
						}
						_ => {
							return Err(anyhow!(
								"invalid template variable '{}'",
								variable
							));
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

	pub async fn create_unit(
		&self, registry_path: PathBuf, volume_root: PathBuf,
	) -> Result<()> {
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

		let client = buckle::systemd::Systemd::new_system().await?;
		client.reload().await?;
		client
			.start(format!(
				"{}.service",
				self.package.title.to_string()
			))
			.await?;

		Ok(())
	}

	pub async fn remove_unit(&self) -> Result<()> {
		let client = buckle::systemd::Systemd::new_system().await?;
		let _ = client
			.stop(format!("{}.service", self.package.title.to_string()))
			.await;
		std::fs::remove_file(self.filename()).map_err(|e| {
			anyhow!(
				"Could not remove service unit {}: {}",
				self.filename().display(),
				e
			)
		})?;

		client.reload().await?;

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::SystemdUnit;
	use crate::{CompiledPackage, Registry, SYSTEMD_SERVICE_ROOT};
	use anyhow::Result;
	use tempfile::TempDir;

	async fn load(
		registry: &Registry, name: &str, version: &str,
	) -> Result<CompiledPackage> {
		registry.load(name, version)?.compile().await
	}

	#[tokio::test]
	async fn unit_names() {
		let registry = Registry::new("testdata/registry".into());
		let unit = SystemdUnit::new(
			load(&registry, "podman-test", "0.0.2").await.unwrap(),
			Some(SYSTEMD_SERVICE_ROOT.into()),
			Some("/usr/bin/charon".into()),
		);
		assert_eq!(
			unit.filename().as_os_str(),
			"/etc/systemd/system/podman-test-0.0.2.service"
		);

		assert_eq!(unit.service_name(), "podman-test-0.0.2.service");
	}

	#[tokio::test]
	async fn unit_contents() {
		let registry = Registry::new("testdata/registry".into());
		let td = TempDir::new().unwrap();
		let path = td.path();
		let pkg =
			load(&registry, "podman-test", "0.0.2").await.unwrap();
		let unit = SystemdUnit::new(
			pkg,
			Some(crate::SYSTEMD_SERVICE_ROOT.into()),
			Some(crate::DEFAULT_CHARON_BIN_PATH.into()),
		);
		let text = unit
			.unit("testdata/registry".into(), path.to_path_buf())
			.unwrap();
		assert_eq!(
			text,
			format!(
				r#"
[Unit]
Description=Charon launcher for podman-test, version 0.0.2

[Service]
ExecStart=/usr/bin/charon -r testdata/registry launch podman-test 0.0.2 {}
ExecStop=/usr/bin/charon -r testdata/registry stop podman-test 0.0.2 {}
Restart=always
TimeoutSec=300

[Install]
Alias=podman-test-0.0.2.service
"#,
				path.display(),
				path.display()
			)
		);
	}
}
