use super::*;
use crate::*;
use anyhow::Result;

fn string_vec(v: Vec<&str>) -> Vec<String> {
	v.iter().map(ToString::to_string).collect::<Vec<String>>()
}

async fn load(
	registry: &Registry, name: &str, version: &str,
) -> Result<CompiledPackage> {
	registry.load(name, version)?.compile().await
}

mod livetests {
	use super::*;
	use std::{
		os::unix::{fs::MetadataExt, process::ExitStatusExt},
		process::Stdio,
	};
	use tempfile::{NamedTempFile, TempDir};

	#[tokio::test]
	async fn test_downloader() {
		let tf = NamedTempFile::new().unwrap();
		let path = tf.path();

		download_vm_image(
			"file://testdata/ubuntu.img",
			path.to_path_buf(),
		)
		.unwrap();
		let md = path.metadata().unwrap();
		// it should be as big as a machine image, this is
		// lower than the size of the current image in the makefile
		assert!(md.size() > 240 * 1024 * 1024);

		// just a file over http. this should be small and accessible.
		download_vm_image(
            "https://raw.githubusercontent.com/curl/curl/refs/heads/master/lib/file.c",
            path.to_path_buf(),
        )
        .unwrap();
		let md = path.metadata().unwrap();
		// this check ensures we downloaded something new to the same path, truncating it, by
		// making sure it's small.
		assert!(md.size() < 1024 * 1024);
		assert!(md.size() > 1024);
	}

	#[tokio::test]
	async fn launch_podman() {
		let registry = Registry::new("testdata/registry".into());
		let td = TempDir::new().unwrap();
		let path = td.path();

		let args = generate_command(
			load(&registry, "podman-test", "0.0.2").await.unwrap(),
			path.to_path_buf(),
		)
		.unwrap();

		let mut child = std::process::Command::new(&args[0])
			.args(args.iter().skip(1))
			.spawn()
			.unwrap();

		assert!(child.id() != 0);
		assert!(unsafe {
			libc::kill(child.id() as i32, libc::SIGINT) == 0
		});
		let status = child.wait().unwrap();
		assert!(status.signal().unwrap() as i32 == libc::SIGINT);

		let pkg =
			load(&registry, "podman-test", "0.0.3").await.unwrap();
		let args =
			generate_command(pkg.clone(), path.to_path_buf()).unwrap();

		let _ = stop_package(pkg.clone(), path.to_path_buf());

		let mut child = std::process::Command::new(&args[0])
			.args(args.iter().skip(1))
			.stdout(Stdio::null())
			.stderr(Stdio::null())
			.spawn()
			.unwrap();
		assert!(child.id() != 0);

		// wait up to 60s for the container to boot by checking the exposed port
		let start = std::time::Instant::now();

		let mut found = false;
		'check: while std::time::Instant::now() - start
			< std::time::Duration::from_secs(60)
		{
			match tokio::net::TcpStream::connect("127.0.0.1:8000").await
			{
				Ok(_) => {
					found = true;
					break 'check;
				}
				Err(_) => {
					tokio::time::sleep(std::time::Duration::from_secs(
						1,
					))
					.await
				}
			}
		}

		assert!(found);

		// request a webpage from the nginx container, should be good
		let resp = reqwest::get("http://localhost:8000").await.unwrap();
		assert_eq!(resp.status(), 200);

		stop_package(pkg, path.to_path_buf()).unwrap();
		let status = child.wait().unwrap();
		assert!(status.success());
	}

	//
	// #[test]
	// fn launch_qemu() {
	//     let registry = Registry::new("testdata/registry".into());
	//     let args = generate_command(
	//         load(&registry, "plex-qemu", "0.0.2").await.unwrap(),
	//         "testdata/volume-root".into(),
	//     )
	//     .unwrap();
	//     let child = std::process::Command::new(&args[0])
	//         .args(args.iter().skip(1))
	//         .spawn();
	// }
}

mod cli_generation {
	use super::*;

	#[tokio::test]
	async fn qemu_cli() {
		let registry = Registry::new("testdata/registry".into());
		assert_eq!(
			generate_command(
				load(&registry, "plex-qemu", "0.0.2").await.unwrap(),
				"/volume-root".into()
			)
			.unwrap(),
			string_vec(vec![
				QEMU_COMMAND,
				"-nodefaults",
				"-chardev",
				"socket,server=on,wait=off,id=char0,path=/volume-root/qemu-monitor",
				"-mon",
				"chardev=char0,mode=control,pretty=on",
				"-machine",
				"accel=kvm",
				"-vga",
				"none",
				"-m",
				"8192M",
				"-cpu",
				"max",
				"-smp",
				"cpus=4,cores=4,maxcpus=4",
				"-nic",
				"user",
				"-drive",
				"driver=raw,if=virtio,file=/volume-root/image,cache=none,media=disk,index=0",
				"-drive",
				"driver=raw,if=virtio,file=/volume-root/test,cache=none,media=disk,index=1"
			]),
		);

		assert_eq!(
			generate_command(
				load(&registry, "plex-qemu", "0.0.1").await.unwrap(),
				"/volume-root".into()
			)
			.unwrap(),
			string_vec(vec![
				QEMU_COMMAND,
				"-nodefaults",
				"-chardev",
				"socket,server=on,wait=off,id=char0,path=/volume-root/qemu-monitor",
				"-mon",
				"chardev=char0,mode=control,pretty=on",
				"-machine",
				"accel=kvm",
				"-vga",
				"none",
				"-m",
				"4096M",
				"-cpu",
				"max",
				"-smp",
				"cpus=8,cores=8,maxcpus=8",
				"-nic",
				"user,hostfwd=tcp:0.0.0.0:1234-:5678,hostfwd=tcp:0.0.0.0:2345-:6789",
				"-drive",
				"driver=raw,if=virtio,file=/volume-root/image,cache=none,media=disk,index=0"
			]),
		);
	}

	#[tokio::test]
	async fn podman_cli() {
		let registry = Registry::new("testdata/registry".into());
		assert_eq!(
			generate_command(
				load(&registry, "plex", "0.0.2").await.unwrap(),
				"/volume-root".into()
			)
			.unwrap(),
			string_vec(vec![
				PODMAN_COMMAND,
				"run",
				"--rm",
				"--name",
				"plex-0.0.2",
				"scratch"
			])
		);
		assert_eq!(
			generate_command(
				load(&registry, "plex", "0.0.1").await.unwrap(),
				"/volume-root".into()
			)
			.unwrap(),
			string_vec(vec![
				PODMAN_COMMAND,
				"run",
				"--rm",
				"--name",
				"plex-0.0.1",
				"scratch"
			])
		);
		assert_eq!(
			generate_command(
				load(&registry, "podman-test", "0.0.1").await.unwrap(),
				"/volume-root".into()
			)
			.unwrap(),
			string_vec(vec![
				PODMAN_COMMAND,
				"run",
				"--rm",
				"--name",
				"podman-test-0.0.1",
				"-v",
				"/volume-root/private:/private-test:rprivate",
				"-v",
				"/volume-root/shared:/shared-test:rshared",
				"--pid",
				"host",
				"--network",
				"host",
				"--privileged",
				"--cap-add",
				"SYS_ADMIN",
				"docker://debian"
			])
		);
	}
}
