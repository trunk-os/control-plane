use crate::{
	grpc::{
		GrpcLogDirection, GrpcLogMessage, GrpcLogParams, GrpcPortForward, GrpcProtocol,
		GrpcUnitName, GrpcUnitSettings, PingResult, UnitEnabledState, UnitListFilter,
		UnitRuntimeState, ZfsListFilter, ZfsName,
		network_client::NetworkClient as GRPCNetworkClient,
		status_client::StatusClient as GRPCStatusClient,
		systemd_client::SystemdClient as GRPCSystemdClient, zfs_client::ZfsClient as GRPCZfsClient,
	},
	systemd::{LogDirection, Unit, UnitSettings},
	upnp::Protocol,
};
// we expose these types we should serve them
pub use crate::{
	sysinfo::Info,
	zfs::{Dataset, ModifyDataset, ModifyVolume, Volume, ZFSStat},
};
use std::path::PathBuf;
use tonic::{Request, Streaming, transport::Channel};

type Result<T> = std::result::Result<T, tonic::Status>;

#[derive(Debug, Clone)]
pub struct Client {
	socket: PathBuf,
}

pub struct NetworkClient {
	client: GRPCNetworkClient<Channel>,
}

pub struct StatusClient {
	client: GRPCStatusClient<Channel>,
}

pub struct ZFSClient {
	client: GRPCZfsClient<Channel>,
}

pub struct SystemdClient {
	client: GRPCSystemdClient<Channel>,
}

impl Client {
	pub fn new(socket: PathBuf) -> anyhow::Result<Self> {
		Ok(Self { socket })
	}

	pub async fn network(&self) -> anyhow::Result<NetworkClient> {
		let client =
			GRPCNetworkClient::connect(format!("unix://{}", self.socket.to_str().unwrap())).await?;
		Ok(NetworkClient { client })
	}

	pub async fn status(&self) -> anyhow::Result<StatusClient> {
		let client =
			GRPCStatusClient::connect(format!("unix://{}", self.socket.to_str().unwrap())).await?;
		Ok(StatusClient { client })
	}

	pub async fn zfs(&self) -> anyhow::Result<ZFSClient> {
		let client =
			GRPCZfsClient::connect(format!("unix://{}", self.socket.to_str().unwrap())).await?;
		Ok(ZFSClient { client })
	}

	pub async fn systemd(&self) -> anyhow::Result<SystemdClient> {
		let client =
			GRPCSystemdClient::connect(format!("unix://{}", self.socket.to_str().unwrap())).await?;
		Ok(SystemdClient { client })
	}
}

impl NetworkClient {
	pub async fn expose_port(&mut self, port: u16, protocol: Protocol, name: String) -> Result<()> {
		let protocol: GrpcProtocol = protocol.into();
		self.client
			.expose_port(tonic::Request::new(GrpcPortForward {
				port: port.into(),
				protocol: protocol.into(),
				name,
			}))
			.await?;
		Ok(())
	}
}

impl SystemdClient {
	pub async fn start_unit(&mut self, name: String) -> Result<()> {
		self.client
			.start_unit(Request::new(GrpcUnitName { name }))
			.await?;
		Ok(())
	}

	pub async fn stop_unit(&mut self, name: String) -> Result<()> {
		self.client
			.stop_unit(Request::new(GrpcUnitName { name }))
			.await?;
		Ok(())
	}

	pub async fn unit_info(&mut self, name: String) -> Result<Unit> {
		let unit = self
			.client
			.unit_info(Request::new(GrpcUnitName { name }))
			.await?;
		Ok(unit.into_inner().into())
	}

	pub async fn reload(&mut self) -> Result<()> {
		self.client.reload(()).await?;
		Ok(())
	}

	pub async fn list(&mut self, filter: Option<String>) -> Result<Vec<Unit>> {
		let filter = UnitListFilter {
			filter: filter.unwrap_or_default(),
		};

		let units = self.client.list(Request::new(filter)).await?.into_inner();
		let mut v = Vec::new();
		for unit in units.items {
			v.push(unit.into())
		}

		Ok(v)
	}

	pub async fn set_unit(&mut self, unit: UnitSettings) -> Result<()> {
		let out = GrpcUnitSettings {
			name: unit.name,
			enabled_state: Into::<UnitEnabledState>::into(unit.enabled_state).into(),
			runtime_state: Into::<UnitRuntimeState>::into(unit.runtime_state).into(),
		};
		self.client.set_unit(Request::new(out)).await?;
		Ok(())
	}

	pub async fn unit_log(
		&mut self, name: &str, count: usize, cursor: Option<String>,
		direction: Option<LogDirection>,
	) -> Result<Streaming<GrpcLogMessage>> {
		let resp = self
			.client
			.unit_log(GrpcLogParams {
				name: name.to_string(),
				count: count as u64,
				cursor: cursor.unwrap_or_default(),
				direction: Into::<GrpcLogDirection>::into(direction.unwrap_or_default()).into(),
			})
			.await?
			.into_inner();
		Ok(resp)
	}
}

impl StatusClient {
	pub async fn ping(&mut self) -> Result<PingResult> {
		Ok(self.client.ping(Request::new(())).await?.into_inner())
	}
}

impl ZFSClient {
	pub async fn root_path(&mut self) -> Result<String> {
		self.client
			.root_path(Request::new(()))
			.await
			.map(|x| x.into_inner().root)
	}

	pub async fn create_dataset(&mut self, dataset: Dataset) -> Result<()> {
		self.client
			.create_dataset(Request::new(dataset.into()))
			.await?;
		Ok(())
	}

	pub async fn create_volume(&mut self, volume: Volume) -> Result<()> {
		self.client
			.create_volume(Request::new(volume.into()))
			.await?;
		Ok(())
	}

	pub async fn modify_dataset(&mut self, dataset: ModifyDataset) -> Result<()> {
		self.client
			.modify_dataset(Request::new(dataset.into()))
			.await?;
		Ok(())
	}

	pub async fn modify_volume(&mut self, volume: ModifyVolume) -> Result<()> {
		self.client
			.modify_volume(Request::new(volume.into()))
			.await?;
		Ok(())
	}

	pub async fn list(&mut self, filter: Option<String>) -> Result<Vec<ZFSStat>> {
		Ok(self
			.client
			.list(Request::new(ZfsListFilter { filter }))
			.await?
			.into_inner()
			.into())
	}

	pub async fn destroy(&mut self, name: String) -> Result<()> {
		self.client.destroy(Request::new(ZfsName { name })).await?;
		Ok(())
	}
}
