use easy_upnp::{PortMappingProtocol, UpnpConfig};
use serde::{Deserialize, Serialize};

use crate::grpc::{GrpcPortForward, GrpcProtocol};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum Protocol {
	#[default]
	TCP,
	UDP,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PortForward {
	pub port: u16,
	pub protocol: Protocol,
}

impl From<GrpcPortForward> for PortForward {
	fn from(value: GrpcPortForward) -> Self {
		Self {
			port: value.port as u16,
			protocol: value.protocol().into(),
		}
	}
}

impl From<GrpcProtocol> for Protocol {
	fn from(value: GrpcProtocol) -> Self {
		match value {
			GrpcProtocol::Tcp => Self::TCP,
			GrpcProtocol::Udp => Self::UDP,
		}
	}
}

impl Into<GrpcProtocol> for Protocol {
	fn into(self) -> GrpcProtocol {
		match self {
			Self::TCP => GrpcProtocol::Tcp,
			Self::UDP => GrpcProtocol::Udp,
		}
	}
}

impl Into<PortMappingProtocol> for Protocol {
	fn into(self) -> PortMappingProtocol {
		match self {
			Protocol::TCP => PortMappingProtocol::TCP,
			Protocol::UDP => PortMappingProtocol::UDP,
		}
	}
}

impl Into<UpnpConfig> for PortForward {
	fn into(self) -> UpnpConfig {
		UpnpConfig {
			address: None,
			port: self.port,
			protocol: self.protocol.into(),
			duration: 30,
			comment: "Forward for Trunk Package".into(),
		}
	}
}
