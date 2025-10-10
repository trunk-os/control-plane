use crate::{
	Config, Global, GlobalRegistry, PromptCollection, PromptResponses, ProtoLastRunState,
	ProtoLoadState, ProtoPackageTitle, ProtoRuntimeState, ProtoStatus, ProtoUninstallData,
	ResponseRegistry, SystemdUnit, TemplatedInput, proto_package_installed::ProtoInstallState,
};
use anyhow::{Result, anyhow};
use buckle::{
	client::{Dataset as ZfsDataset, Volume as ZfsVolume},
	systemd::{LastRunState, LoadState, RuntimeState},
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

//
// something really important to understand about this code is that the TemplatedInput type is only
// really constrained by the concrete types it leverages. Everything else in input.rs is basically
// just enough bullshit to keep rust happy. A proc macro would make this much simpler, and I will
// get to that eventually.
//

pub(crate) const PACKAGE_SUBPATH: &str = "packages";
pub(crate) const INSTALLED_SUBPATH: &str = "installed";

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourcePackage {
	pub title: PackageTitle,
	pub description: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub dependencies: Option<Vec<PackageTitle>>,
	pub source: Source,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub networking: Option<Networking>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub storage: Option<Storage>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub system: Option<System>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub resources: Option<Resources>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub prompts: Option<PromptCollection>,
	#[serde(skip)]
	pub root: Option<std::path::PathBuf>,
}

impl PartialOrd for SourcePackage {
	#[inline]
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		Some(self.cmp(other))
	}
}

impl Ord for SourcePackage {
	#[inline]
	fn cmp(&self, other: &Self) -> std::cmp::Ordering {
		self.title.cmp(&other.title)
	}
}

impl SourcePackage {
	pub fn from_file(root: &Path, name: &str, version: &str) -> Result<Self> {
		let pb = root
			.join(PACKAGE_SUBPATH)
			.join(name)
			.join(format!("{}.json", version));
		let mut res: Self =
			serde_json::from_reader(std::fs::OpenOptions::new().read(true).open(pb).map_err(
				|e| {
					anyhow!(
						"Error loading {}/{} package definition: {}",
						name,
						version,
						e
					)
				},
			)?)
			.map_err(|e| {
				anyhow!(
					"Error parsing JSON in {}/{} package definition: {}",
					name,
					version,
					e
				)
			})?;
		res.root = Some(root.to_path_buf());
		Ok(res)
	}

	#[inline]
	pub fn globals(&self) -> Result<Global> {
		if self.root.is_none() {
			return Err(anyhow!(
				"source package does not contain registry information, cannot find globals"
			));
		}

		let registry = GlobalRegistry {
			root: self.root.clone().unwrap().clone(),
		};

		registry.get(&self.title.name)
	}

	#[inline]
	pub fn response_registry(&self) -> Result<ResponseRegistry> {
		if self.root.is_none() {
			return Err(anyhow!(
				"source package does not contain registry information, cannot find responses"
			));
		}

		Ok(ResponseRegistry::new(self.root.clone().unwrap().clone()))
	}

	#[inline]
	pub fn set_responses(&self, responses: &PromptResponses) -> Result<()> {
		self.response_registry()?.set(&self.title.name, responses)
	}

	#[inline]
	pub fn responses(&self) -> Result<PromptResponses> {
		self.response_registry()?.get(&self.title.name)
	}

	pub async fn compile(&self) -> Result<CompiledPackage> {
		let globals = self.globals()?;
		let prompts = self.prompts.clone().unwrap_or_default();
		let responses = self.responses().unwrap_or_default();

		Ok(CompiledPackage {
			root: self.root.clone().unwrap_or_default(),
			title: self.title.clone(),
			description: self.description.clone(),
			dependencies: self.dependencies.clone().unwrap_or_default(),
			source: self.source.compile(&globals, &prompts, &responses)?,
			networking: self
				.networking
				.clone()
				.unwrap_or_default()
				.compile(&globals, &prompts, &responses)?,
			storage: self
				.storage
				.clone()
				.unwrap_or_default()
				.compile(&globals, &prompts, &responses)?,
			system: self
				.system
				.clone()
				.unwrap_or_default()
				.compile(&globals, &prompts, &responses)?,
			resources: self
				.resources
				.clone()
				.unwrap_or_default()
				.compile(&globals, &prompts, &responses)?,
		})
	}

	pub fn dependencies(&self) -> Result<Vec<SourcePackage>> {
		// FIXME: this check probably shouldn't exist
		if self.root.is_none() {
			return Err(anyhow!(
				"source package does not contain registry information, cannot find dependencies"
			));
		}

		let mut v = Vec::new();

		for item in self.dependencies.clone().unwrap_or_default() {
			v.push(Self::from_file(
				self.root.clone().unwrap().as_path(),
				&item.name,
				&item.version,
			)?)
		}

		Ok(v)
	}
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompiledPackage {
	pub title: PackageTitle,
	pub description: String,
	pub dependencies: Vec<PackageTitle>,
	pub source: CompiledSource,
	pub networking: CompiledNetworking,
	pub storage: CompiledStorage,
	pub system: CompiledSystem,
	pub resources: CompiledResources,

	root: PathBuf,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum InstallStatus {
	Installed(buckle::systemd::Status),
	NotInstalled,
}

impl From<InstallStatus> for ProtoInstallState {
	fn from(value: InstallStatus) -> Self {
		match &value {
			InstallStatus::Installed(status) => Self::Installed(ProtoStatus {
				load_state: match status.load_state {
					LoadState::Loaded => ProtoLoadState::Loaded,
					LoadState::Unloaded => ProtoLoadState::Unloaded,
					LoadState::Inactive => ProtoLoadState::Inactive,
				}
				.into(),
				runtime_state: match status.runtime_state {
					RuntimeState::Started => ProtoRuntimeState::Started,
					RuntimeState::Stopped => ProtoRuntimeState::Stopped,
					RuntimeState::Reloaded => ProtoRuntimeState::Reloaded,
					RuntimeState::Restarted => ProtoRuntimeState::Restarted,
				}
				.into(),
				last_run_state: match status.last_run_state {
					LastRunState::Dead => ProtoLastRunState::Dead,
					LastRunState::Failed => ProtoLastRunState::Failed,
					LastRunState::Exited => ProtoLastRunState::Exited,
					LastRunState::Active => ProtoLastRunState::Active,
					LastRunState::Mounted => ProtoLastRunState::Mounted,
					LastRunState::Running => ProtoLastRunState::Running,
					LastRunState::Plugged => ProtoLastRunState::Plugged,
					LastRunState::Waiting => ProtoLastRunState::Waiting,
					LastRunState::Listening => ProtoLastRunState::Listening,
				}
				.into(),
			}),
			InstallStatus::NotInstalled => Self::NotInstalled(()),
		}
	}
}

impl From<ProtoInstallState> for InstallStatus {
	fn from(value: ProtoInstallState) -> Self {
		match &value {
			ProtoInstallState::Installed(status) => Self::Installed(buckle::systemd::Status {
				load_state: match status.load_state() {
					ProtoLoadState::Loaded => LoadState::Loaded,
					ProtoLoadState::Unloaded => LoadState::Unloaded,
					ProtoLoadState::Inactive => LoadState::Inactive,
				},
				runtime_state: match status.runtime_state() {
					ProtoRuntimeState::Started => RuntimeState::Started,
					ProtoRuntimeState::Stopped => RuntimeState::Stopped,
					ProtoRuntimeState::Reloaded => RuntimeState::Reloaded,
					ProtoRuntimeState::Restarted => RuntimeState::Restarted,
				},
				last_run_state: match status.last_run_state() {
					ProtoLastRunState::Dead => LastRunState::Dead,
					ProtoLastRunState::Failed => LastRunState::Failed,
					ProtoLastRunState::Exited => LastRunState::Exited,
					ProtoLastRunState::Active => LastRunState::Active,
					ProtoLastRunState::Mounted => LastRunState::Mounted,
					ProtoLastRunState::Running => LastRunState::Running,
					ProtoLastRunState::Plugged => LastRunState::Plugged,
					ProtoLastRunState::Waiting => LastRunState::Waiting,
					ProtoLastRunState::Listening => LastRunState::Listening,
				},
			}),
			ProtoInstallState::NotInstalled(_) => Self::NotInstalled,
		}
	}
}

impl CompiledPackage {
	pub fn systemd_unit(
		&self, config: Config, systemd_root: Option<PathBuf>, charon_path: Option<PathBuf>,
	) -> SystemdUnit {
		SystemdUnit::new(
			// FIXME: remove this expect when the time is right
			config.buckle().expect("Could not connect to buckle"),
			self.clone(),
			systemd_root,
			charon_path,
		)
	}

	fn installed_path(&self) -> PathBuf {
		self.root
			.join(INSTALLED_SUBPATH)
			.join(&self.title.name)
			.join(&self.title.version)
	}

	pub async fn install(&self) -> Result<()> {
		let pb = self.root.join(INSTALLED_SUBPATH).join(&self.title.name);
		std::fs::create_dir_all(&pb)?;

		std::fs::OpenOptions::new()
			.create_new(true)
			.truncate(true)
			.write(true)
			.open(self.installed_path())?;

		Ok(())
	}

	pub async fn uninstall(&self) -> Result<()> {
		std::fs::remove_file(self.installed_path())?;
		Ok(())
	}

	pub async fn installed(&self) -> Result<InstallStatus> {
		if std::fs::exists(self.installed_path())? {
			let client = buckle::systemd::Systemd::new_system().await?;
			let path = client.get_unit(format!("{}.service", self.title)).await?;
			let status = client.status(path).await?;
			Ok(InstallStatus::Installed(status))
		} else {
			Ok(InstallStatus::NotInstalled)
		}
	}

	pub async fn provision(&self, buckle_socket: &Path) -> Result<()> {
		let client = buckle::client::Client::new(buckle_socket.to_path_buf())?;

		client
			.zfs()
			.await?
			.create_dataset(ZfsDataset {
				name: self.title.name.clone(),
				quota: None,
			})
			.await?;

		for volume in &self.storage.volumes {
			if volume.mountpoint.is_some() {
				client
					.zfs()
					.await?
					.create_dataset(ZfsDataset {
						name: format!("{}/{}", self.title.name, volume.name),
						quota: Some(volume.size),
					})
					.await?;
			} else {
				client
					.zfs()
					.await?
					.create_volume(ZfsVolume {
						name: format!("{}/{}", self.title.name, volume.name),
						size: volume.size,
					})
					.await?;
			}
		}

		Ok(())
	}

	pub async fn deprovision(&self, buckle_socket: &Path) -> Result<()> {
		let client = buckle::client::Client::new(buckle_socket.to_path_buf())?;

		let unit_name = format!("{}.service", self.title.to_string());

		let _ = client.systemd().await?.stop_unit(unit_name.clone()).await;

		'wait: loop {
			match client.systemd().await?.unit_info(unit_name.clone()).await {
				Ok(status) => match status.status.last_run_state {
					LastRunState::Dead | LastRunState::Failed | LastRunState::Exited => {
						break 'wait;
					}
					_ => {}
				},
				Err(_) => break 'wait,
			}
			tokio::time::sleep(std::time::Duration::from_millis(100)).await;
		}

		for volume in &self.storage.volumes {
			client
				.zfs()
				.await?
				.destroy(format!("{}/{}", self.title.name, volume.name))
				.await?;
		}

		client.zfs().await?.destroy(self.title.name.clone()).await?;

		Ok(())
	}
}

impl From<ProtoPackageTitle> for PackageTitle {
	fn from(value: ProtoPackageTitle) -> Self {
		Self {
			name: value.name,
			version: value.version,
		}
	}
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackageTitle {
	pub name: String,
	pub version: String,
}

impl PackageTitle {
	pub fn format_volume(&self, root: &Path) -> PathBuf {
		root.join(&self.name)
	}
}

impl std::fmt::Display for PackageTitle {
	#[inline]
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(&format!("{}-{}", self.name, self.version))
	}
}

impl PartialOrd for PackageTitle {
	#[inline]
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		Some(self.cmp(other))
	}
}

impl Ord for PackageTitle {
	#[inline]
	fn cmp(&self, other: &Self) -> std::cmp::Ordering {
		match self.name.cmp(&other.name) {
			std::cmp::Ordering::Equal => self.version.cmp(&other.version),
			x => x,
		}
	}
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackageStatus {
	pub title: PackageTitle,
	pub installed: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum Source {
	#[serde(rename = "url")]
	URL(TemplatedInput<String>),
	#[serde(rename = "container")]
	Container(TemplatedInput<String>),
}

impl Default for Source {
	#[inline]
	fn default() -> Self {
		Self::Container("scratch".parse().unwrap())
	}
}

impl Source {
	pub fn compile(
		&self, globals: &Global, prompts: &PromptCollection, responses: &PromptResponses,
	) -> Result<CompiledSource> {
		Ok(match self {
			Self::URL(x) => CompiledSource::URL(x.output(globals, prompts, responses)?),
			Self::Container(x) => CompiledSource::Container(x.output(globals, prompts, responses)?),
		})
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum CompiledSource {
	#[serde(rename = "url")]
	URL(String),
	#[serde(rename = "container")]
	Container(String),
}

impl Default for CompiledSource {
	#[inline]
	fn default() -> Self {
		Self::Container("scratch".parse().unwrap())
	}
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Networking {
	#[serde(skip_serializing_if = "Option::is_none")]
	pub forward_ports: Option<Vec<(TemplatedInput<u16>, TemplatedInput<u16>)>>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub expose_ports: Option<Vec<(TemplatedInput<u16>, TemplatedInput<u16>)>>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub internal_network: Option<TemplatedInput<String>>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub hostname: Option<TemplatedInput<String>>,
}

impl Networking {
	pub fn compile(
		&self, globals: &Global, prompts: &PromptCollection, responses: &PromptResponses,
	) -> Result<CompiledNetworking> {
		let mut forward_ports = Vec::new();
		if let Some(fp) = &self.forward_ports {
			for port in fp {
				forward_ports.push((
					port.0.output(globals, prompts, responses)?,
					port.1.output(globals, prompts, responses)?,
				));
			}
		}

		let mut expose_ports = Vec::new();
		if let Some(ep) = &self.expose_ports {
			for port in ep {
				expose_ports.push((
					port.0.output(globals, prompts, responses)?,
					port.1.output(globals, prompts, responses)?,
				));
			}
		}

		let internal_network = if let Some(internal_network) = self
			.internal_network
			.as_ref()
			.map(|x| x.output(globals, prompts, responses))
		{
			match internal_network {
				Ok(x) => Some(x),
				Err(e) => return Err(e),
			}
		} else {
			None
		};

		let hostname = if let Some(hostname) = self
			.hostname
			.as_ref()
			.map(|x| x.output(globals, prompts, responses))
		{
			match hostname {
				Ok(x) => Some(x),
				Err(e) => return Err(e),
			}
		} else {
			None
		};

		Ok(CompiledNetworking {
			forward_ports,
			expose_ports,
			internal_network,
			hostname,
		})
	}
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompiledNetworking {
	pub forward_ports: Vec<(u16, u16)>,
	pub expose_ports: Vec<(u16, u16)>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub internal_network: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub hostname: Option<String>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Storage {
	pub volumes: Vec<Volume>,
}

impl Storage {
	pub fn compile(
		&self, globals: &Global, prompts: &PromptCollection, responses: &PromptResponses,
	) -> Result<CompiledStorage> {
		let mut v = Vec::new();
		for volume in &self.volumes {
			v.push(volume.compile(globals, prompts, responses)?);
		}

		Ok(CompiledStorage { volumes: v })
	}
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompiledStorage {
	pub volumes: Vec<CompiledVolume>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Volume {
	pub name: TemplatedInput<String>,
	pub size: TemplatedInput<u64>,
	pub mountpoint: Option<TemplatedInput<String>>,
	pub recreate: TemplatedInput<bool>,
	pub private: TemplatedInput<bool>,
}

impl Volume {
	pub fn compile(
		&self, globals: &Global, prompts: &PromptCollection, responses: &PromptResponses,
	) -> Result<CompiledVolume> {
		let mountpoint = if let Some(mountpoint) = self
			.mountpoint
			.as_ref()
			.map(|x| x.output(globals, prompts, responses))
		{
			match mountpoint {
				Ok(x) => Some(x),
				Err(e) => return Err(e),
			}
		} else {
			None
		};

		Ok(CompiledVolume {
			name: self.name.output(globals, prompts, responses)?,
			size: self.size.output(globals, prompts, responses)?,
			mountpoint,
			recreate: self.recreate.output(globals, prompts, responses)?,
			private: self.private.output(globals, prompts, responses)?,
		})
	}
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompiledVolume {
	pub name: String,
	pub size: u64,
	pub mountpoint: Option<String>,
	pub recreate: bool,
	pub private: bool,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct System {
	// --pid host
	pub host_pid: TemplatedInput<bool>,
	// --net host
	pub host_net: TemplatedInput<bool>,
	pub capabilities: Vec<TemplatedInput<String>>,
	pub privileged: TemplatedInput<bool>,
}

impl System {
	pub fn compile(
		&self, globals: &Global, prompts: &PromptCollection, responses: &PromptResponses,
	) -> Result<CompiledSystem> {
		let mut capabilities = Vec::new();

		for cap in &self.capabilities {
			capabilities.push(cap.output(globals, prompts, responses)?);
		}

		Ok(CompiledSystem {
			host_pid: self.host_pid.output(globals, prompts, responses)?,
			host_net: self.host_net.output(globals, prompts, responses)?,
			capabilities,
			privileged: self.privileged.output(globals, prompts, responses)?,
		})
	}
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompiledSystem {
	// --pid host
	pub host_pid: bool,
	// --net host
	pub host_net: bool,
	pub capabilities: Vec<String>,
	pub privileged: bool,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Resources {
	pub cpus: TemplatedInput<u64>,
	pub memory: TemplatedInput<u64>,
	// probably something to bring in PCI devices to appease the crypto folks
}

impl Resources {
	pub fn compile(
		&self, globals: &Global, prompts: &PromptCollection, responses: &PromptResponses,
	) -> Result<CompiledResources> {
		Ok(CompiledResources {
			cpus: self.cpus.output(globals, prompts, responses)?,
			memory: self.memory.output(globals, prompts, responses)?,
		})
	}
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompiledResources {
	pub cpus: u64,
	pub memory: u64,
	// probably something to bring in PCI devices to appease the crypto folks
}

pub struct Registry {
	root: PathBuf,
}

impl Registry {
	pub fn new(root: PathBuf) -> Self {
		Self { root }
	}

	pub fn path(&self) -> PathBuf {
		self.root.clone()
	}

	pub fn list(&self) -> Result<Vec<PackageStatus>> {
		let installed = self.installed()?;

		let mut v = Vec::new();

		let mut items = std::fs::read_dir(self.root.join(PACKAGE_SUBPATH))?
			.filter_map(|x| {
				if let Ok(x) = x
					&& x.metadata().ok()?.is_dir()
				{
					return Some(x.path());
				}

				None
			})
			.collect::<Vec<PathBuf>>();

		items.sort();

		for item in &items {
			let name = item.file_name().unwrap().to_str().unwrap();
			let mut inner = std::fs::read_dir(item)?
				.filter_map(|x| {
					if let Ok(x) = x
						&& x.metadata().ok()?.is_file()
					{
						return Some(x.path());
					}

					None
				})
				.collect::<Vec<PathBuf>>();

			inner.sort();

			for item in inner.iter().rev().collect::<Vec<&PathBuf>>() {
				let version = item.file_stem().unwrap().to_str().unwrap();

				let title = PackageTitle {
					name: name.to_string(),
					version: version.to_string(),
				};

				let is_installed = installed
					.iter()
					.find(|x| x.name == title.name && x.version == title.version)
					.is_some();
				v.push(PackageStatus {
					title,
					installed: is_installed,
				})
			}
		}

		Ok(v)
	}

	pub fn installed(&self) -> Result<Vec<PackageTitle>> {
		let mut v = Vec::new();

		let items = std::fs::read_dir(self.root.join(INSTALLED_SUBPATH))?;

		for item in items {
			let item = item?;
			if item.metadata()?.is_dir() {
				let path = item.path();
				let name = path.file_name().unwrap().to_str().unwrap();
				let inner = std::fs::read_dir(item.path())?;

				for item in inner {
					let item = item?;
					if item.metadata()?.is_file() {
						let path = item.path();
						let version = path.file_name().unwrap().to_str().unwrap();

						v.push(PackageTitle {
							name: name.to_string(),
							version: version.to_string(),
						});
					}
				}
			}
		}

		Ok(v)
	}

	pub fn response_registry(&self) -> ResponseRegistry {
		ResponseRegistry::new(self.root.clone())
	}

	pub fn validate(&self, name: &str, version: &str) -> Result<()> {
		let package = self.load(name, version)?;

		if package.title.name != name || package.title.version != version {
			return Err(anyhow!("Invalid name or version"));
		}

		// validate we can load globals, but we don't need them
		let _ = package.globals()?;

		let dependencies = package.dependencies.clone().unwrap_or_default();

		// validate package dependencies exist
		for item in &dependencies {
			self.validate(&item.name, &item.version)?;
		}

		Ok(())
	}

	pub fn remove(&self, name: &str) -> Result<()> {
		Ok(std::fs::remove_dir_all(
			self.root.join(PACKAGE_SUBPATH).join(name),
		)?)
	}

	pub fn load(&self, name: &str, version: &str) -> Result<SourcePackage> {
		SourcePackage::from_file(&self.root, name, version)
	}

	pub fn write(&self, package: &SourcePackage) -> Result<()> {
		let pb = self.root.join(PACKAGE_SUBPATH).join(&package.title.name);
		std::fs::create_dir_all(&pb)?;

		let name = pb.join(format!("{}.json.tmp", package.title.version));
		let f = std::fs::OpenOptions::new()
			.create(true)
			.truncate(true)
			.write(true)
			.open(&name)?;

		serde_json::to_writer_pretty(&f, &package)?;

		Ok(std::fs::rename(
			&name,
			pb.join(format!("{}.json", package.title.version)),
		)?)
	}

	#[inline]
	pub fn globals(&self, package: &SourcePackage) -> Result<Global> {
		package.globals()
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UninstallData {
	pub name: String,
	pub version: String,
	pub purge: bool,
}

impl From<ProtoUninstallData> for UninstallData {
	fn from(value: ProtoUninstallData) -> Self {
		Self {
			name: value.name,
			version: value.version,
			purge: value.purge,
		}
	}
}

#[cfg(test)]
mod tests {
	use crate::{
		CompiledPackage, Global, GlobalRegistry, PackageTitle, Registry, SourcePackage, Variables,
	};

	#[test]
	fn dependencies() {
		let registry = Registry::new("testdata/registry".into());
		let pkg = registry.load("with-dependencies", "0.0.1").unwrap();
		let plex = registry.load("plex", "0.0.2").unwrap();
		let deps = pkg.dependencies().unwrap();

		assert_eq!(deps, vec![plex]);
	}

	#[test]
	fn validate() {
		let registry = Registry::new("testdata/registry".into());
		assert!(registry.validate("plex", "0.0.1").is_ok());
		assert!(registry.validate("plex", "0.0.2").is_ok());

		// doesn't exist
		assert!(registry.validate("plex", "0.0.3").is_err());
		// doesn't have a variables json
		assert!(registry.validate("no-variables", "0.0.1").is_err());

		assert!(registry.validate("with-dependencies", "0.0.1").is_ok());

		// depends on a non-existent version of plex
		assert!(registry.validate("bad-dependencies", "0.0.1").is_err());
		// depends on non-existent package
		assert!(registry.validate("bad-dependencies", "0.0.2").is_err());
		// depends on a bad package
		assert!(registry.validate("bad-dependencies", "0.0.3").is_err());
		// invalid name, valid version
		assert!(registry.validate("bad-name-version", "0.0.1").is_err());
		// invalid version, valid name
		assert!(registry.validate("bad-name-version", "0.0.2").is_err());
	}

	#[test]
	fn io() {
		let dir = tempfile::tempdir().unwrap();
		let table = &[SourcePackage {
			title: PackageTitle {
				name: "plex".into(),
				version: "1.2.3".into(),
			},
			root: Some(dir.path().to_path_buf()),
			..Default::default()
		}];

		let pr = Registry {
			root: dir.path().to_path_buf(),
		};

		for item in table {
			assert!(pr.write(item).is_ok());
			let cmp = pr.load(&item.title.name, &item.title.version).unwrap();
			assert_eq!(item.clone(), cmp);
		}
	}

	#[test]
	fn globals() {
		let dir = tempfile::tempdir().unwrap();
		let packages = &[SourcePackage {
			title: PackageTitle {
				name: "plex".into(),
				version: "1.2.3".into(),
			},
			root: Some(dir.path().to_path_buf()),
			..Default::default()
		}];

		let pr = Registry {
			root: dir.path().to_path_buf(),
		};

		for item in packages {
			pr.write(item).unwrap();
		}

		let mut variables = Variables::default();
		variables.insert("foo".into(), "bar".into());
		variables.insert("baz".into(), "quux".into());

		let globals = &[Global {
			name: "plex".into(),
			variables: variables.clone(),
		}];

		let gr = GlobalRegistry {
			root: dir.path().to_path_buf(),
		};

		for item in globals {
			assert!(gr.set(item).is_ok());
		}

		assert_eq!(pr.globals(&packages[0]).unwrap(), globals[0]);
	}

	#[tokio::test]
	async fn compile() {
		let dir = tempfile::tempdir().unwrap();
		let packages = &[SourcePackage {
			title: PackageTitle {
				name: "plex".into(),
				version: "1.2.3".into(),
			},
			root: Some(dir.path().to_path_buf()),
			..Default::default()
		}];

		let pr = Registry {
			root: dir.path().to_path_buf(),
		};

		for item in packages {
			pr.write(item).unwrap();
		}

		let mut variables = Variables::default();
		variables.insert("foo".into(), "bar".into());
		variables.insert("baz".into(), "quux".into());

		let globals = &[Global {
			name: "plex".into(),
			variables: variables.clone(),
		}];

		let gr = GlobalRegistry {
			root: dir.path().to_path_buf(),
		};

		for item in globals {
			assert!(gr.set(item).is_ok());
		}

		let pkg = pr.load("plex", "1.2.3").unwrap();
		let out = pkg.compile().await.unwrap();

		assert_eq!(
			out,
			CompiledPackage {
				root: dir.path().to_path_buf(),
				title: PackageTitle {
					name: "plex".into(),
					version: "1.2.3".into(),
				},
				..Default::default()
			}
		);
	}
}
