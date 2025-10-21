use std::error::Error;

use anyhow::anyhow;

pub async fn command(
	bin: &'static str, args: Vec<&'static str>,
) -> Result<(String, String), Box<dyn Error>> {
	let output = std::process::Command::new(bin).args(&args).output()?;
	let stdout = String::from_utf8_lossy(&output.stdout).to_string();
	let stderr = String::from_utf8_lossy(&output.stderr).to_string();

	if output.status.success() {
		Ok((stdout, stderr))
	} else {
		Err(anyhow!(
			"podman command `{:?}` had an error [status: {}] - stdout: {} / stderr: {}",
			args,
			output.status.code().unwrap_or_default(),
			stdout,
			stderr,
		)
		.into())
	}
}

pub async fn podman(args: Vec<&'static str>) -> Result<(), Box<dyn Error>> {
	command("podman", args).await?;
	Ok(())
}

pub async fn zfs(args: Vec<&'static str>) -> Result<(), Box<dyn Error>> {
	command("zfs", args).await?;
	Ok(())
}

pub async fn zpool(args: Vec<&'static str>) -> Result<(), Box<dyn Error>> {
	command("zfs", args).await?;
	Ok(())
}
