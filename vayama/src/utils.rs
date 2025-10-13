use std::collections::HashMap;

use anyhow::{Result, anyhow};

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

#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct SystemdServiceUnit {
	name: String,
	sections: HashMap<String, HashMap<String, String>>,
}

impl SystemdServiceUnit {
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

	pub fn write(&self) -> Result<()> {
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn systemd_unit_generate() {
		let mut unit = SystemdServiceUnit {
			name: "test-unit".into(),
			sections: Default::default(),
		};

		let mut unit_section = HashMap::new();
		unit_section.insert("Name".to_string(), "test-unit.service".into());
		unit_section.insert("Description".to_string(), "a test service".into());

		let mut service_section = HashMap::new();
		service_section.insert("Exec".to_string(), "/usr/games/fortune".into());
		service_section.insert("KillMode".to_string(), "pid".into());
		service_section.insert("Restart".to_string(), "always".into());

		let mut install_section = HashMap::new();
		install_section.insert("Alias".to_string(), "also-a-test-unit.service".into());
		install_section.insert("WantedBy".to_string(), "default.target".into());

		unit.sections.insert("Unit".to_string(), unit_section);
		unit.sections.insert("Service".to_string(), service_section);
		unit.sections.insert("Install".to_string(), install_section);

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
