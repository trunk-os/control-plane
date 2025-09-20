use crate::config::LogLevel;
use crate::grpc::{status_client::StatusClient, systemd_client::SystemdClient};
use crate::server::Server;
use anyhow::Result;
use std::sync::LazyLock;
use std::time::Duration;
use tempfile::NamedTempFile;
use tonic::transport::Channel;

pub const BUCKLE_TEST_ZPOOL_PREFIX: &str = "buckle-test";

pub static DEFAULT_CONFIG: LazyLock<crate::config::Config> =
	LazyLock::new(|| crate::config::Config {
		socket: "/tmp/buckled.sock".into(),
		zfs: crate::config::ZFSConfig {
			pool: format!("{}-default", BUCKLE_TEST_ZPOOL_PREFIX),
		},
		log_level: LogLevel::Error,
	});

pub fn find_listener() -> Result<std::path::PathBuf> {
	std::fs::create_dir_all("tmp/sockets")?;
	let file = NamedTempFile::new_in("tmp/sockets")?;
	let (_, path) = file.keep()?;
	Ok(path)
}

pub async fn make_server(config: Option<crate::config::Config>) -> Result<std::path::PathBuf> {
	let mut config = config.unwrap_or_else(|| DEFAULT_CONFIG.clone());
	config.socket = find_listener()?;
	let server = Server::new_with_config(Some(config.clone()));

	tokio::spawn(async move { server.start().unwrap().await.unwrap() });

	// wait for server to start
	tokio::time::sleep(Duration::from_millis(100)).await;

	Ok(config.socket)
}

pub async fn get_status_client(socket: std::path::PathBuf) -> Result<StatusClient<Channel>> {
	Ok(StatusClient::connect(format!("unix://{}", socket.to_str().unwrap())).await?)
}

pub async fn get_systemd_client(socket: std::path::PathBuf) -> Result<SystemdClient<Channel>> {
	Ok(SystemdClient::connect(format!("unix://{}", socket.to_str().unwrap())).await?)
}

use crate::grpc::zfs_client::ZfsClient;
pub async fn get_zfs_client(socket: std::path::PathBuf) -> Result<ZfsClient<Channel>> {
	Ok(ZfsClient::connect(format!("unix://{}", socket.to_str().unwrap())).await?)
}

// FIXME these commands should accept Option<&str>, setting the name to "default" when None. This
// would match the default zpool configuration setup for the server.
use anyhow::anyhow;
pub fn create_zpool(name: &str) -> Result<(String, String)> {
	std::fs::create_dir_all("tmp")?;

	let (_, path) = tempfile::NamedTempFile::new_in("tmp")?.keep()?;

	if !std::process::Command::new("truncate")
		.args(vec!["-s", "5G", path.to_str().unwrap()])
		.stdout(std::io::stdout())
		.stderr(std::io::stderr())
		.status()?
		.success()
	{
		return Err(anyhow!("Could not grow file for zpool"));
	}

	let name = format!("{}-{}", BUCKLE_TEST_ZPOOL_PREFIX, name);
	if !std::process::Command::new("zpool")
		.args(vec!["create", &name, path.to_str().unwrap()])
		.stdout(std::io::stdout())
		.stderr(std::io::stderr())
		.status()?
		.success()
	{
		return Err(anyhow!("could not create zpool '{}'", name));
	}

	Ok((name, path.to_string_lossy().to_string()))
}

pub fn destroy_zpool(name: &str, file: Option<&str>) -> Result<()> {
	let name = format!("{}-{}", BUCKLE_TEST_ZPOOL_PREFIX, name);
	if !std::process::Command::new("zpool")
		.args(vec!["destroy", "-f", &name])
		.stdout(std::io::stdout())
		.stderr(std::io::stderr())
		.status()?
		.success()
	{
		return Err(anyhow!("could not destroy zpool: {}", name));
	}

	if let Some(file) = file {
		return Ok(std::fs::remove_file(file)?);
	}

	Ok(())
}

pub fn list_zpools() -> Result<Vec<String>> {
	let out = std::process::Command::new("zpool")
		.args(vec!["list"])
		.stderr(std::io::stderr())
		.output()?;
	if out.status.success() {
		let out = String::from_utf8(out.stdout)?;
		let lines = out.split('\n');

		let mut ret = Vec::new();

		for line in lines.skip(1) {
			let mut name = String::new();
			for ch in line.chars() {
				if ch != ' ' {
					name.push(ch)
				} else {
					break;
				}
			}
			ret.push(name);
		}

		return Ok(ret);
	}

	Err(anyhow!("error listing zpools"))
}

mod tests {
	mod zfs {
		#[allow(unused)]
		use super::super::{BUCKLE_TEST_ZPOOL_PREFIX, create_zpool, destroy_zpool, list_zpools};

		#[test]
		fn create_remove_zpool() {
			let _ = destroy_zpool("testutil-test", None);
			let (_, file) = create_zpool("testutil-test").unwrap();
			assert!(file.len() > 0);
			assert!(
				list_zpools()
					.unwrap()
					.contains(&format!("{}-testutil-test", BUCKLE_TEST_ZPOOL_PREFIX))
			);
			destroy_zpool("testutil-test", Some(&file)).unwrap();
			assert!(!std::fs::exists(file).unwrap())
		}
	}
}
