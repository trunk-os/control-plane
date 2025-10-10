use crate::{
	grpc::{
		GrpcLogMessage, GrpcLogParams, GrpcUnitList, GrpcUnitName, GrpcUnitSettings, PingResult,
		UnitListFilter, ZfsDataset, ZfsExists, ZfsList, ZfsListFilter, ZfsModifyDataset,
		ZfsModifyVolume, ZfsName, ZfsRoot, ZfsVolume,
		status_server::{Status, StatusServer},
		systemd_server::{Systemd, SystemdServer},
		zfs_server::{Zfs, ZfsServer},
	},
	sysinfo::Info,
};
use std::{fs::Permissions, os::unix::fs::PermissionsExt, pin::Pin};
use tokio_stream::{Stream, wrappers::ReceiverStream};
use tonic::{Request, Response, Result, transport::Server as TransportServer};
use tonic_middleware::MiddlewareLayer;
use tracing::info;

// FIXME needs a way to shut down
#[derive(Debug, Default, Clone)]
pub struct Server {
	config: crate::config::Config,
}

impl Server {
	pub fn new_with_config(config: Option<crate::config::Config>) -> Self {
		match config {
			Some(config) => Self { config },
			None => Self::default(),
		}
	}

	pub fn start(
		&self,
	) -> anyhow::Result<impl std::future::Future<Output = Result<(), tonic::transport::Error>>> {
		info!("Starting service.");

		if let Some(parent) = self.config.socket.to_path_buf().parent() {
			std::fs::create_dir_all(parent)?;
		}

		if std::fs::exists(&self.config.socket)? {
			std::fs::remove_file(&self.config.socket)?;
		}

		let uds = tokio::net::UnixListener::bind(&self.config.socket)?;
		let uds_stream = tokio_stream::wrappers::UnixListenerStream::new(uds);

		std::fs::set_permissions(&self.config.socket, Permissions::from_mode(0o600))?;

		Ok(TransportServer::builder()
			.layer(MiddlewareLayer::new(crate::middleware::LogMiddleware))
			.add_service(StatusServer::new(self.clone()))
			.add_service(ZfsServer::new(self.clone()))
			.add_service(SystemdServer::new(self.clone()))
			.serve_with_incoming(uds_stream))
	}
}

#[tonic::async_trait]
impl Systemd for Server {
	async fn start_unit(&self, req: tonic::Request<GrpcUnitName>) -> Result<Response<()>> {
		Ok(Response::new(
			crate::systemd::Systemd::new_system()
				.await
				.map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?
				.start(req.into_inner().name)
				.await
				.map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?,
		))
	}

	async fn reload(&self, _: tonic::Request<()>) -> Result<Response<()>> {
		Ok(Response::new(
			crate::systemd::Systemd::new_system()
				.await
				.map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?
				.reload()
				.await
				.map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?,
		))
	}

	async fn list(&self, filter: Request<UnitListFilter>) -> Result<Response<GrpcUnitList>> {
		let systemd = crate::systemd::Systemd::new_system()
			.await
			.map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?;
		let mut v = Vec::new();
		let filter = filter.into_inner();

		let mut out = None;
		if !filter.filter.is_empty() {
			out = Some(filter.filter);
		}

		for item in systemd
			.list(out)
			.await
			.map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?
		{
			v.push(item.into());
		}

		Ok(Response::new(GrpcUnitList { items: v }))
	}

	type UnitLogStream = Pin<Box<dyn Stream<Item = Result<GrpcLogMessage>> + Send>>;

	async fn set_unit(&self, _filter: Request<GrpcUnitSettings>) -> Result<Response<()>> {
		Ok(Response::new(()))
	}

	// FIXME: this really is only a streaming method because of memory usage concerns. Maybe
	// another way would be better
	async fn unit_log(
		&self, params: Request<GrpcLogParams>,
	) -> Result<Response<Self::UnitLogStream>> {
		let params = params.into_inner();
		let (tx, rx) = tokio::sync::mpsc::channel(params.count as usize);
		let output_stream = ReceiverStream::new(rx);
		let systemd = crate::systemd::Systemd::new_system()
			.await
			.map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?;

		let p2 = params.clone();
		tokio::spawn(async move {
			let params = p2;
			let mut rcv = systemd
				.log(&params.name, params.count as usize, None, None)
				.await
				.unwrap();
			while let Some(items) = rcv.recv().await {
				let mut time: Option<std::time::SystemTime> = None;
				let mut msg: Option<String> = None;
				let mut pid: Option<u64> = None;
				let mut cursor: Option<String> = None;

				for (key, value) in items {
					match key.as_str() {
						"_SOURCE_REALTIME_TIMESTAMP" => {
							time = Some(
								std::time::SystemTime::UNIX_EPOCH
									+ std::time::Duration::from_secs(value.parse::<u64>().unwrap()),
							)
						}
						"MESSAGE" => msg = Some(value),
						"_PID" => pid = Some(value.parse().unwrap()),
						"CURSOR" => cursor = Some(value),
						_ => {}
					}

					if time.is_some() && msg.is_some() && pid.is_some() {
						tx.send(Ok(GrpcLogMessage {
							service_name: params.name.clone(),
							msg: msg.clone().unwrap(),
							pid: pid.unwrap(),
							time: time.map(Into::into),
							cursor: cursor.unwrap(),
						}))
						.await
						.unwrap();
						time = None;
						msg = None;
						pid = None;
						cursor = None;
					}
				}
			}
		});

		Ok(Response::new(Box::pin(output_stream) as Self::UnitLogStream))
	}
}

#[tonic::async_trait]
impl Status for Server {
	async fn ping(&self, _: Request<()>) -> Result<Response<PingResult>> {
		Ok(Response::new(PingResult {
			info: Some(Info::default().into()),
		}))
	}
}

#[tonic::async_trait]
impl Zfs for Server {
	async fn root_path(&self, _: Request<()>) -> Result<Response<ZfsRoot>> {
		Ok(Response::new(ZfsRoot {
			root: format!("/{}", self.config.zfs.pool),
		}))
	}

	async fn exists(&self, name: Request<ZfsName>) -> Result<Response<ZfsExists>> {
		let n = name.into_inner();
		let items = self
			.config
			.zfs
			.controller()
			.list(Some(n.clone().name))
			.map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?;

		if let Some(found_name) = items.first().map(|x| &x.name) {
			Ok(Response::new(ZfsExists {
				exists: &n.name == found_name,
			}))
		} else {
			Ok(Response::new(ZfsExists { exists: false }))
		}
	}

	async fn modify_dataset(&self, info: Request<ZfsModifyDataset>) -> Result<Response<()>> {
		self.config
			.zfs
			.controller()
			.modify_dataset(info.into_inner().into())
			.map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?;
		Ok(Response::new(()))
	}

	async fn modify_volume(&self, info: Request<ZfsModifyVolume>) -> Result<Response<()>> {
		self.config
			.zfs
			.controller()
			.modify_volume(info.into_inner().into())
			.map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?;
		Ok(Response::new(()))
	}

	async fn list(&self, filter: Request<ZfsListFilter>) -> Result<Response<ZfsList>> {
		let list = self
			.config
			.zfs
			.controller()
			.list(filter.get_ref().filter.clone())
			.map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?;
		return Ok(Response::new(list.into()));
	}

	async fn create_dataset(&self, dataset: Request<ZfsDataset>) -> Result<Response<()>> {
		self.config
			.zfs
			.controller()
			.create_dataset(&dataset.into_inner().into())
			.map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?;

		return Ok(Response::new(()));
	}

	async fn create_volume(&self, volume: Request<ZfsVolume>) -> Result<Response<()>> {
		self.config
			.zfs
			.controller()
			.create_volume(&volume.into_inner().into())
			.map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?;
		return Ok(Response::new(()));
	}

	async fn destroy(&self, name: Request<ZfsName>) -> Result<Response<()>> {
		self.config
			.zfs
			.controller()
			.destroy(name.get_ref().name.clone())
			.map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?;
		return Ok(Response::new(()));
	}
}

#[cfg(test)]
mod tests {
	mod systemd {
		use tokio_stream::StreamExt;

		use crate::{
			grpc::{GrpcLogDirection, GrpcLogParams},
			testutil::{get_systemd_client, make_server},
		};

		#[tokio::test]
		async fn test_log() {
			let mut client = get_systemd_client(make_server(None).await.unwrap())
				.await
				.unwrap();
			let log = client
				.unit_log(GrpcLogParams {
					name: "network.target".into(),
					count: 100,
					cursor: "".into(),
					direction: GrpcLogDirection::Forward.into(),
				})
				.await
				.unwrap();

			let mut log = log.into_inner();
			let mut total = 0;

			while let Some(item) = log.next().await {
				let item = item.unwrap();
				assert!(!item.msg.is_empty());
				assert!(item.time.is_some());
				assert_ne!(!item.time.unwrap().seconds, 0);
				assert_ne!(item.pid, 0);
				assert!(!item.cursor.is_empty());
				total += 1;
			}

			assert!(total < 100);
			assert!(total > 0);
		}
	}

	mod status {
		use crate::testutil::{get_status_client, make_server};

		#[tokio::test]
		async fn test_ping() {
			let mut client = get_status_client(make_server(None).await.unwrap())
				.await
				.unwrap();
			let results = client
				.ping(tonic::Request::new(()))
				.await
				.unwrap()
				.into_inner();
			assert!(results.info.is_some());
			let info = results.info.unwrap();
			assert_ne!(info.uptime, 0);
			assert_ne!(info.available_memory, 0);
			assert_ne!(info.total_memory, 0);
			assert_ne!(info.cpus, 0);
			assert!(!info.host_name.is_empty());
			assert!(!info.kernel_version.is_empty());
			assert_ne!(info.load_average, [0.0, 0.0, 0.0]);
			assert_ne!(info.processes, 0);
		}
	}

	mod zfs {
		use crate::{
			grpc::{
				ZfsDataset, ZfsListFilter, ZfsModifyDataset, ZfsModifyVolume, ZfsName, ZfsType,
				ZfsVolume,
			},
			testutil::{
				BUCKLE_TEST_ZPOOL_PREFIX, create_zpool, destroy_zpool, get_zfs_client, make_server,
			},
		};

		#[tokio::test]
		async fn test_zfs_operations() {
			let _ = destroy_zpool("default", None);
			let (_, file) = create_zpool("default").unwrap();
			let mut client = get_zfs_client(make_server(None).await.unwrap())
				.await
				.unwrap();

			let res = client
				.list(tonic::Request::new(ZfsListFilter::default()))
				.await
				.unwrap();

			assert_eq!(res.into_inner().entries.len(), 0);

			client
				.create_dataset(tonic::Request::new(
					ZfsDataset {
						name: "dataset".to_string(),
						..Default::default()
					}
					.into(),
				))
				.await
				.unwrap();

			let res = client
				.list(tonic::Request::new(ZfsListFilter::default()))
				.await
				.unwrap()
				.into_inner()
				.entries;

			assert_eq!(res.len(), 1);

			let item = &res[0];

			assert_eq!(item.kind(), ZfsType::Dataset);
			assert_eq!(item.name, "dataset");
			assert_eq!(
				item.full_name,
				format!("{}-default/dataset", BUCKLE_TEST_ZPOOL_PREFIX),
			);
			assert_ne!(item.size, 0);
			assert_ne!(item.used, 0);
			assert_ne!(item.refer, 0);
			assert_ne!(item.avail, 0);
			assert_eq!(
				item.mountpoint,
				Some(format!("/{}-default/dataset", BUCKLE_TEST_ZPOOL_PREFIX))
			);

			client
				.create_volume(tonic::Request::new(
					ZfsVolume {
						name: "volume".to_string(),
						size: 100 * 1024 * 1024,
					}
					.into(),
				))
				.await
				.unwrap();

			let res = client
				.list(tonic::Request::new(ZfsListFilter::default()))
				.await
				.unwrap()
				.into_inner()
				.entries;

			assert_eq!(res.len(), 2);

			let res = client
				.list(tonic::Request::new(ZfsListFilter {
					filter: Some("dataset".to_string()),
				}))
				.await
				.unwrap()
				.into_inner()
				.entries;

			assert_eq!(res.len(), 1);

			let item = &res[0];

			assert_eq!(item.kind(), ZfsType::Dataset);
			assert_eq!(item.name, "dataset");
			assert_eq!(
				item.full_name,
				format!("{}-default/dataset", BUCKLE_TEST_ZPOOL_PREFIX),
			);
			assert_ne!(item.size, 0);
			assert_ne!(item.used, 0);
			assert_ne!(item.refer, 0);
			assert_ne!(item.avail, 0);
			assert_eq!(
				item.mountpoint,
				Some(format!("/{}-default/dataset", BUCKLE_TEST_ZPOOL_PREFIX))
			);

			client
				.modify_dataset(tonic::Request::new(ZfsModifyDataset {
					name: "dataset".into(),
					modifications: Some(ZfsDataset {
						name: "dataset2".into(),
						quota: Some(5 * 1024 * 1024),
					}),
				}))
				.await
				.unwrap();

			let res = client
				.list(tonic::Request::new(ZfsListFilter {
					filter: Some("dataset2".to_string()),
				}))
				.await
				.unwrap()
				.into_inner()
				.entries;

			assert_eq!(res.len(), 1);

			let item = &res[0];

			assert_eq!(item.kind(), ZfsType::Dataset);
			assert_eq!(item.name, "dataset2");
			assert_eq!(
				item.full_name,
				format!("{}-default/dataset2", BUCKLE_TEST_ZPOOL_PREFIX),
			);
			assert_ne!(item.size, 0);
			assert_ne!(item.used, 0);
			assert_ne!(item.refer, 0);
			assert_ne!(item.avail, 0);
			assert_eq!(
				item.mountpoint,
				Some(format!("/{}-default/dataset2", BUCKLE_TEST_ZPOOL_PREFIX))
			);

			let res = client
				.list(tonic::Request::new(ZfsListFilter {
					filter: Some("volume".to_string()),
				}))
				.await
				.unwrap()
				.into_inner()
				.entries;

			assert_eq!(res.len(), 1);

			let item = &res[0];

			assert_eq!(item.kind(), ZfsType::Volume);
			assert_eq!(item.name, "volume");
			assert_eq!(
				item.full_name,
				format!("{}-default/volume", BUCKLE_TEST_ZPOOL_PREFIX),
			);
			assert_ne!(item.size, 0);
			assert_ne!(item.used, 0);
			assert_ne!(item.refer, 0);
			assert_ne!(item.avail, 0);
			assert_eq!(item.mountpoint, None);

			client
				.modify_volume(tonic::Request::new(ZfsModifyVolume {
					name: "volume".into(),
					modifications: Some(ZfsVolume {
						name: "volume2".into(),
						size: 5 * 1024 * 1024,
					}),
				}))
				.await
				.unwrap();

			let res = client
				.list(tonic::Request::new(ZfsListFilter {
					filter: Some("volume2".to_string()),
				}))
				.await
				.unwrap()
				.into_inner()
				.entries;

			assert_eq!(res.len(), 1);

			let item = &res[0];

			assert_eq!(item.kind(), ZfsType::Volume);
			assert_eq!(item.name, "volume2");
			assert_eq!(
				item.full_name,
				format!("{}-default/volume2", BUCKLE_TEST_ZPOOL_PREFIX),
			);
			assert_ne!(item.size, 0);
			assert!(
				item.size < 6 * 1024 * 1024 && item.size > 4 * 1024 * 1024,
				"{}",
				item.size
			);
			assert_ne!(item.used, 0);
			assert_ne!(item.refer, 0);
			assert_ne!(item.avail, 0);
			assert_eq!(item.mountpoint, None);

			let mut passed = false;

			for _ in 0..10 {
				passed = client
					.destroy(tonic::Request::new(ZfsName {
						name: "volume2".to_string(),
					}))
					.await
					.is_ok();
				if passed {
					break;
				}
			}

			assert!(passed);

			let res = client
				.list(tonic::Request::new(ZfsListFilter {
					filter: Some("volume2".to_string()),
				}))
				.await
				.unwrap()
				.into_inner()
				.entries;

			assert_eq!(res.len(), 0);

			client
				.destroy(tonic::Request::new(ZfsName {
					name: "dataset2".to_string(),
				}))
				.await
				.unwrap();

			let res = client
				.list(tonic::Request::new(ZfsListFilter {
					filter: Some("dataset2".to_string()),
				}))
				.await
				.unwrap()
				.into_inner()
				.entries;

			assert_eq!(res.len(), 0);

			let res = client
				.list(tonic::Request::new(ZfsListFilter::default()))
				.await
				.unwrap()
				.into_inner()
				.entries;

			assert_eq!(res.len(), 0);

			destroy_zpool("default", Some(&file)).unwrap();
		}
	}
}
