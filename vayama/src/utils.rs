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
			String::from_utf8(output.stderr)?
		))
	}
}

pub async fn podman(args: Vec<&str>) -> Result<String> {
	generic_command(PODMAN_COMMAND, args).await
}

pub async fn zfs(args: Vec<&str>) -> Result<String> {
	generic_command(ZFS_COMMAND, args).await
}
