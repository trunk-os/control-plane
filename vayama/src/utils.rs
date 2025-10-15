use anyhow::{Result, anyhow};
use std::{collections::HashMap, io::Write, path::PathBuf};

const PODMAN_COMMAND: &str = "podman";
const ZFS_COMMAND: &str = "zfs";

pub async fn generic_command(command: &str, args: Vec<&str>) -> Result<String> {
	let output = tokio::process::Command::new(command)
		.args(&args)
		.output()
		.await?;

	if output.status.success() {
		Ok(String::from_utf8(output.stdout)?)
	} else {
		Err(anyhow!(
			"command `{}` [args: {:?}] exited with status {:?}: stderr: [{}]",
			command,
			args,
			output.status.code(),
			String::from_utf8_lossy(&output.stderr).to_string()
		))
	}
}

pub async fn podman(args: Vec<&str>) -> Result<String> {
	generic_command(PODMAN_COMMAND, args).await
}

pub async fn zfs(args: Vec<&str>) -> Result<String> {
	generic_command(ZFS_COMMAND, args).await
}

#[macro_export]
macro_rules! systemd_unit {
	($name:expr, $(($section_name:expr, ($(($key:expr, $value:expr),)*)),)*) => {
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
	name: String,
	sections: HashMap<String, HashMap<String, String>>,
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

	pub fn write(&self, root: Option<PathBuf>) -> Result<()> {
		let out = self.generate()?;
		let mut f = std::fs::OpenOptions::new()
			.create(true)
			.write(true)
			.truncate(true)
			// FIXME: the root path here should be more flexible
			.open(
				root.unwrap_or(PathBuf::from("/etc/systemd/system"))
					.join(&format!("{}.service", self.name)),
			)?;

		Ok(f.write_all(out.as_bytes())?)
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
					("Name", "test-unit.service"),
					("Description", "a test service"),
				)
			),
			(
				"Install",
				(
					("Alias", "also-a-test-unit.service"),
					("WantedBy", "default.target"),
				)
			),
			(
				"Service",
				(
					("Exec", "/usr/games/fortune"),
					("KillMode", "pid"),
					("Restart", "always"),
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
