use anyhow::{Result, anyhow};
use std::{collections::HashMap, io::Write, path::PathBuf};

use crate::migration::MigrationError;

const PODMAN_COMMAND: &str = "podman";
const ZFS_COMMAND: &str = "zfs";
const SYSTEMCTL_COMMAND: &str = "systemctl";

pub async fn command(cmd: &str, args: Vec<&str>) -> Result<(String, String), MigrationError> {
	let output = tokio::process::Command::new(cmd)
		.args(&args)
		.output()
		.await
		.map_err(|e| anyhow!(e.to_string()))?;

	if output.status.success() {
		Ok((
			String::from_utf8(output.stdout).map_err(|e| anyhow!(e.to_string()))?,
			String::from_utf8(output.stderr).map_err(|e| anyhow!(e.to_string()))?,
		))
	} else {
		Err(MigrationError::Command(
			format!("{} {}", cmd, args.join(" ")),
			String::from_utf8_lossy(&output.stderr).to_string(),
			output.status.code().unwrap_or_default(),
		))
	}
}

pub async fn podman(args: Vec<&str>) -> Result<(String, String), MigrationError> {
	command(PODMAN_COMMAND, args).await
}

pub async fn zfs(args: Vec<&str>) -> Result<(String, String), MigrationError> {
	command(ZFS_COMMAND, args).await
}

pub async fn systemctl(args: Vec<&str>) -> Result<(String, String), MigrationError> {
	command(SYSTEMCTL_COMMAND, args).await
}

#[macro_export]
macro_rules! systemd_unit {
	($name:expr, $(($section_name:expr, ($(($key:expr => $value:expr),)*)),)*) => {
    {
        let mut unit = SystemdServiceUnit {
            name: $name.into(),
            ..Default::default()
        };

        $(
            unit.add_section($section_name.into(), [$(($key.into(), $value.into()),)*]);
        )*

        unit
    }
  };
}

#[derive(Debug, Clone, Default)]
pub struct SystemdServiceUnit {
	pub name: String,
	pub sections: HashMap<String, HashMap<String, String>>,
}

impl SystemdServiceUnit {
	pub fn add_section<const N: usize>(&mut self, name: String, section: [(String, String); N]) {
		self.sections.insert(name, HashMap::from(section));
	}

	pub fn generate(&self) -> Result<String> {
		let mut out = String::new();

		let mut sections = self
			.sections
			.keys()
			.map(Clone::clone)
			.collect::<Vec<String>>();
		sections.sort();

		for name in &sections {
			out += &format!("[{}]\n", name);

			let mut subsections = self.sections[name]
				.keys()
				.map(Clone::clone)
				.collect::<Vec<String>>();
			subsections.sort();

			for key in &subsections {
				out += &format!("{}={}\n", key, self.sections[name][key]);
			}

			out += "\n"
		}

		Ok(out)
	}

	pub fn write(&self, root: Option<PathBuf>) -> Result<(), MigrationError> {
		let out = self.generate()?;
		let filename = root
			.unwrap_or(PathBuf::from("/etc/systemd/system"))
			.join(&format!("{}.service", self.name));
		let mut f = std::fs::OpenOptions::new()
			.create(true)
			.write(true)
			.truncate(true)
			.open(filename.clone())
			.map_err(|e| MigrationError::WriteFile(filename.clone(), e.to_string()))?;

		Ok(f.write_all(out.as_bytes())
			.map_err(|e| MigrationError::WriteFile(filename, e.to_string()))?)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn systemd_unit_generate() {
		let unit = systemd_unit!(
			"test-unit",
			(
				"Unit",
				(
					("Name" => "test-unit.service"),
					("Description" => "a test service"),
				)
			),
			(
				"Install",
				(
					("Alias" => "also-a-test-unit.service"),
					("WantedBy" => "default.target"),
				)
			),
			(
				"Service",
				(
					("Exec" => "/usr/games/fortune"),
					("KillMode" => "pid"),
					("Restart" => "always"),
				)
			),
		);

		assert_eq!(
			unit.generate().unwrap().trim(),
			r#"
[Install]
Alias=also-a-test-unit.service
WantedBy=default.target

[Service]
Exec=/usr/games/fortune
KillMode=pid
Restart=always

[Unit]
Description=a test service
Name=test-unit.service
      "#
			.trim(),
		)
	}
}
