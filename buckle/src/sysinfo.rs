use crate::grpc::SystemInfo;
use fancy_duration::AsFancyDuration;
use serde::{Deserialize, Serialize};
use sysinfo::System;
use tracing::{debug, trace};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Info {
	pub uptime: u64,            // in seconds
	pub available_memory: u64,  // bytes
	pub total_memory: u64,      // bytes
	pub cpus: usize,            // count of cpus
	pub cpu_usage: f32,         // percentage
	pub host_name: String,      // short name
	pub kernel_version: String, // only the version string
	pub load_average: [f64; 3], // 1, 5, 15 min
	pub processes: usize,       // just the count
	pub total_disk: u64,        // bytes
	pub available_disk: u64,    // bytes
}

impl Default for Info {
	fn default() -> Self {
		debug!("Collecting system statistics");
		let time = std::time::Instant::now();
		let s = System::new_all();
		let la = sysinfo::System::load_average();
		let la = [la.one, la.five, la.fifteen];

		let this = Self {
			uptime: sysinfo::System::uptime(),
			available_memory: s.available_memory(),
			total_memory: s.total_memory(),
			cpus: s.cpus().len(),
			cpu_usage: s.global_cpu_usage(),
			host_name: sysinfo::System::host_name()
				.unwrap_or("trunk".into()),
			kernel_version: sysinfo::System::kernel_version()
				.unwrap_or("unknown".into()),
			load_average: la,
			processes: s.processes().len(),
			total_disk: sysinfo::Disks::new_with_refreshed_list()
				.iter()
				.filter(|d| {
					d.name().to_string_lossy().starts_with("trunk")
				})
				.map(|d| d.total_space())
				.reduce(|a, e| a + e)
				.unwrap_or_default(),
			available_disk: sysinfo::Disks::new_with_refreshed_list()
				.iter()
				.filter(|d| {
					d.name().to_string_lossy().starts_with("trunk")
				})
				.map(|d| d.available_space())
				.reduce(|a, e| a + e)
				.unwrap_or_default(),
		};

		trace!(
			"Collecting system statistics took: {}",
			(std::time::Instant::now() - time).fancy_duration(),
		);

		this
	}
}

impl From<SystemInfo> for Info {
	fn from(value: SystemInfo) -> Self {
		Self {
			uptime: value.uptime,
			available_memory: value.available_memory,
			total_memory: value.total_memory,
			cpus: value.cpus as usize,
			cpu_usage: value.cpu_usage,
			host_name: value.host_name,
			kernel_version: value.kernel_version,
			load_average: [
				value.load_average[0],
				value.load_average[1],
				value.load_average[2],
			],
			processes: value.processes as usize,
			total_disk: value.total_disk.into(),
			available_disk: value.available_disk.into(),
		}
	}
}

impl From<Info> for SystemInfo {
	fn from(value: Info) -> Self {
		Self {
			uptime: value.uptime,
			available_memory: value.available_memory,
			total_memory: value.total_memory,
			cpus: value.cpus as u64,
			cpu_usage: value.cpu_usage,
			host_name: value.host_name,
			kernel_version: value.kernel_version,
			load_average: value.load_average.to_vec(),
			processes: value.processes as u64,
			total_disk: value.total_disk.into(),
			available_disk: value.available_disk.into(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn info_defaults() {
		let info = Info::default();
		assert_ne!(info.uptime, 0);
		assert_ne!(info.available_memory, 0);
		assert_ne!(info.total_memory, 0);
		assert_ne!(info.cpus, 0);
		assert_ne!(info.cpu_usage, 0.0);
		assert!(!info.host_name.is_empty());
		assert!(!info.kernel_version.is_empty());
		assert_ne!(info.load_average, [0.0, 0.0, 0.0]);
		assert_ne!(info.processes, 0);
	}
}
