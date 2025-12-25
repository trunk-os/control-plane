use crate::{
	CompiledPackage, CompiledSource,
	qmp::{client::Client, messages::GenericReturn},
};
use anyhow::{Result, anyhow};
use curl::easy::Easy;
use std::{io::Read, process::Stdio};
use std::{
	io::Write,
	path::{Path, PathBuf},
	sync::mpsc::channel,
};

#[cfg(test)]
mod tests;

const PODMAN_COMMAND: &str = "podman";
const QEMU_COMMAND: &str = "qemu-system-x86_64";
const QEMU_IMAGE_FILENAME: &str = "image";
const QEMU_MONITOR_FILENAME: &str = "qemu-monitor";

enum DownloadInfo {
	Data(Vec<u8>),
	#[expect(dead_code)]
	ContentType(String),
	Close,
}

pub fn generate_command(package: CompiledPackage, volume_root: PathBuf) -> Result<Vec<String>> {
	match package.source {
		CompiledSource::URL(_) => generate_vm_command(&package, &volume_root),
		CompiledSource::Container(_) => generate_container_command(&package, &volume_root),
	}
}

pub fn stop_package(package: CompiledPackage, volume_root: PathBuf) -> Result<()> {
	match package.source {
		CompiledSource::URL(_) => vm_shutdown(&package, &volume_root),
		CompiledSource::Container(_) => container_shutdown(&package, &volume_root),
	}
}

pub fn container_shutdown(package: &CompiledPackage, _: &Path) -> Result<()> {
	std::process::Command::new(PODMAN_COMMAND)
		.args(vec!["rm", "-f", &package.title.to_string()])
		.stdout(Stdio::null())
		.stderr(Stdio::null())
		.status()
		.unwrap();
	Ok(())
}

pub fn download_vm_image(u: &str, target: PathBuf) -> Result<()> {
	let parsed: url::Url = u.parse()?;

	// FIXME: all this setup is to facilitate transparent decompression
	//        which of course is not actually implemented yet
	let (s, r) = channel();
	let (close_s, close_r) = channel::<Result<()>>();
	std::thread::spawn(move || {
		let mut f = match std::fs::OpenOptions::new()
			.create(true)
			.truncate(true)
			.write(true)
			.open(&target)
		{
			Ok(f) => f,
			Err(e) => {
				close_s.send(Err(anyhow!(e))).unwrap();
				return;
			}
		};

		while let Ok(item) = r.recv() {
			match item {
				DownloadInfo::Data(data) => {
					if let Err(e) = f.write_all(&data) {
						close_s.send(Err(anyhow!(e))).unwrap();
						return;
					}
				}
				DownloadInfo::ContentType(_) => {}
				DownloadInfo::Close => {
					close_s.send(Ok(())).unwrap();
					return;
				}
			}
		}
	});

	// use a special method for file urls; curl doesn't support them currently
	// https://github.com/alexcrichton/curl-rust/issues/611 tracks this issue.
	if parsed.scheme() == "file" {
		// apparently the url library thinks the first path component of file:// is the host.
		// sigh.
		let mut f = std::fs::OpenOptions::new().read(true).open(format!(
			"{}{}",
			parsed.host().unwrap(),
			parsed.path()
		))?;
		let mut buf: [u8; 4096] = [0u8; 4096];
		loop {
			let size = f.read(&mut buf)?;
			if size == 0 {
				break;
			}
			s.send(DownloadInfo::Data(buf[..size].to_vec())).unwrap();
		}
	} else {
		let mut curl = Easy::new();
		curl.url(u)?;

		let s2 = s.clone();
		curl.header_function(move |header| {
			if let Ok(header) = String::from_utf8(header.into()) {
				let split: Vec<&str> = header.splitn(2, ":").collect();
				if split.len() == 2 && split[0].to_lowercase() == "content-type" {
					s2.send(DownloadInfo::ContentType(split[1].trim().to_string()))
						.unwrap();
				}
			}

			true
		})?;

		let s2 = s.clone();
		curl.write_function(move |data| {
			s2.send(DownloadInfo::Data(data.to_vec())).unwrap();
			Ok(data.len())
		})?;

		curl.perform()?;
	}

	s.send(DownloadInfo::Close)?;
	close_r.recv()??; // this'll catch any error from the thread spawned
	Ok(())
}

fn vm_client(package: &CompiledPackage, volume_root: &Path) -> Result<Client> {
	match Client::new(volume_root.join(QEMU_MONITOR_FILENAME)) {
		Ok(mut us) => {
			us.handshake()?;
			us.send_command::<GenericReturn>("qmp_capabilities", None)?;
			Ok(us)
		}
		Err(_) => Err(anyhow!("{} is not running or not monitored", package.title)),
	}
}

pub fn vm_ping(package: &CompiledPackage, volume_root: &Path) -> Result<()> {
	vm_client(package, volume_root)?;
	Ok(())
}

pub fn vm_shutdown(package: &CompiledPackage, volume_root: &Path) -> Result<()> {
	vm_client(package, volume_root)?.send_command("system_powerdown", None)
}

pub fn vm_quit(package: &CompiledPackage, volume_root: &Path) -> Result<()> {
	vm_client(package, volume_root)?.send_command("quit", None)
}

pub fn generate_vm_command(package: &CompiledPackage, volume_root: &Path) -> Result<Vec<String>> {
	let mut cmd = vec![QEMU_COMMAND.to_string()];

	let mut fwdrules = String::new();
	for (host, guest) in &package.networking.forward_ports {
		fwdrules.push_str(&format!(",hostfwd=tcp:0.0.0.0:{}-:{}", host, guest));
	}

	for (host, guest) in &package.networking.expose_ports {
		fwdrules.push_str(&format!(",hostfwd=tcp:0.0.0.0:{}-:{}", host, guest));
	}

	cmd.append(&mut vec![
		"-nodefaults".into(),
		"-chardev".into(),
		format!(
			"socket,server=on,wait=off,id=char0,path={}",
			volume_root.join(QEMU_MONITOR_FILENAME).display(),
		),
		"-mon".into(),
		"chardev=char0,mode=control,pretty=on".into(),
		"-machine".into(),
		"accel=kvm".into(),
		"-vga".into(),
		"none".into(), // FIXME: move to VNC
		"-m".into(),
		format!("{}M", package.resources.memory),
		"-cpu".into(),
		"max".into(),
		"-smp".into(),
		format!(
			"cpus={},cores={},maxcpus={}",
			package.resources.cpus, package.resources.cpus, package.resources.cpus
		),
		"-nic".into(),
		format!("user{}", fwdrules),
	]);

	cmd.push("-drive".into());
	cmd.push(format!(
		"driver=raw,if=virtio,file={},cache=none,media=disk,index={}",
		volume_root.join(QEMU_IMAGE_FILENAME).display(),
		// NOTE: this offsets the counter below for volumes
		0,
	));

	let excluded_names = [QEMU_IMAGE_FILENAME, QEMU_MONITOR_FILENAME];

	for (x, volume) in package.storage.volumes.iter().enumerate() {
		if excluded_names.contains(&volume.name.as_str()) {
			return Err(anyhow!(
				"VM volumes cannot be named '{}'",
				// this outputs "'foo', or 'bar', or 'baz'"
				excluded_names.join("', or '")
			));
		}

		cmd.push("-drive".to_string());
		cmd.push(format!(
			"driver=raw,if=virtio,file={},cache=none,media=disk,index={}",
			// FIXME formalize making these into files; this doesn't work right yet
			volume_root.join(&volume.name).display(),
			// NOTE: the first drive is above, which is the VM image, which is why this is offset.
			x + 1,
		));
	}

	Ok(cmd)
}

pub fn generate_container_command(
	package: &CompiledPackage, volume_root: &Path,
) -> Result<Vec<String>> {
	let mut cmd = vec![PODMAN_COMMAND.into(), "run".into()];
	let name = package.title.to_string();
	cmd.append(&mut vec!["--rm".into(), "--name".into(), name]);

	if let Some(hostname) = &package.networking.hostname {
		cmd.append(&mut vec!["--hostname".into(), hostname.clone()]);
	}

	// FIXME: solve creating this network in advance
	if let Some(internal_network) = &package.networking.internal_network {
		cmd.append(&mut vec!["--network".into(), internal_network.clone()]);
	}

	for (hostport, localport) in &package.networking.forward_ports {
		let portmap = format!("{}:{}", hostport, localport);
		cmd.append(&mut vec!["-p".into(), portmap]);
	}

	for (hostport, localport) in &package.networking.expose_ports {
		let portmap = format!("{}:{}", hostport, localport);
		cmd.append(&mut vec!["-p".into(), portmap]);
	}

	for volume in &package.storage.volumes {
		// FIXME: create filesystems on block devices.
		//        ignoring them for now
		if let Some(mountpoint) = &volume.mountpoint {
			// FIXME: ignoring volume private flag until I can figure out what's different about zfs on
			// fedora
			// let volmap = if !volume.private {
			// 	format!(
			// 		"{}:{}:rshared",
			// 		volume_root.join(&volume.name).display(),
			// 		mountpoint
			// 	)
			// } else {
			// 	format!(
			// 		"{}:{}:rprivate",
			// 		volume_root.join(&volume.name).display(),
			// 		mountpoint
			// 	)
			// };
			let volmap = format!(
				"{}:{}:rshared",
				volume_root.join(&volume.name).display(),
				mountpoint
			);
			cmd.append(&mut vec!["-v".into(), volmap]);
		}
	}

	let name = if let CompiledSource::Container(name) = &package.source {
		name
	} else {
		return Err(anyhow!("Genuinely curious how you got here, not gonna lie"));
	};

	if package.system.host_pid {
		cmd.append(&mut vec!["--pid".into(), "host".into()]);
	}

	// FIXME: check for this conflict in validate
	if package.system.host_net && package.networking.internal_network.is_none() {
		cmd.append(&mut vec!["--network".into(), "host".into()]);
	}

	if package.system.privileged {
		cmd.append(&mut vec!["--privileged".into()]);
	}

	for cap in &package.system.capabilities {
		cmd.append(&mut vec!["--cap-add".into(), cap.into()]);
	}

	// TODO: cgroups

	cmd.push(name.into());

	Ok(cmd)
}
