use crate::grpc::query_client::QueryClient as GRPCQueryClient;
use crate::grpc::status_client::StatusClient as GRPCStatusClient;
use crate::{grpc::control_client::ControlClient as GRPCControlClient, ProtoPackageTitle};
use crate::{
    InputType, InstallStatus, PackageTitle, Prompt, PromptCollection, PromptResponses,
    ProtoPackageTitleWithRoot, ProtoPromptResponses, ProtoType,
};
use anyhow::Result;
use std::path::PathBuf;
use tonic::{transport::Channel, Request};

#[derive(Debug, Clone)]
pub struct Client {
    socket: PathBuf,
}

pub struct StatusClient {
    client: GRPCStatusClient<Channel>,
}

pub struct ControlClient {
    client: GRPCControlClient<Channel>,
}

pub struct QueryClient {
    client: GRPCQueryClient<Channel>,
}

impl Client {
    pub fn new(socket: PathBuf) -> anyhow::Result<Self> {
        Ok(Self { socket })
    }

    pub async fn status(&self) -> anyhow::Result<StatusClient> {
        let client =
            GRPCStatusClient::connect(format!("unix://{}", self.socket.to_str().unwrap())).await?;
        Ok(StatusClient { client })
    }

    pub async fn control(&self) -> anyhow::Result<ControlClient> {
        let client =
            GRPCControlClient::connect(format!("unix://{}", self.socket.to_str().unwrap())).await?;
        Ok(ControlClient { client })
    }

    pub async fn query(&self) -> anyhow::Result<QueryClient> {
        let client =
            GRPCQueryClient::connect(format!("unix://{}", self.socket.to_str().unwrap())).await?;
        Ok(QueryClient { client })
    }
}

impl StatusClient {
    pub async fn ping(&mut self) -> Result<()> {
        Ok(self.client.ping(Request::new(())).await?.into_inner())
    }
}

impl ControlClient {
    pub async fn install(&mut self, name: &str, version: &str) -> Result<()> {
        Ok(self
            .client
            .install(Request::new(ProtoPackageTitle {
                name: name.to_string(),
                version: version.to_string(),
            }))
            .await?
            .into_inner())
    }

    pub async fn uninstall(&mut self, name: &str, version: &str) -> Result<()> {
        Ok(self
            .client
            .uninstall(Request::new(ProtoPackageTitle {
                name: name.to_string(),
                version: version.to_string(),
            }))
            .await?
            .into_inner())
    }

    pub async fn installed(&mut self, name: &str, version: &str) -> Result<Option<InstallStatus>> {
        let reply = self
            .client
            .installed(Request::new(ProtoPackageTitle {
                name: name.to_string(),
                version: version.to_string(),
            }))
            .await?
            .into_inner();

        Ok(reply.proto_install_state.map(|x| x.into()))
    }

    pub async fn write_unit(
        &mut self,
        name: &str,
        version: &str,
        volume_root: PathBuf,
    ) -> Result<()> {
        let out = ProtoPackageTitleWithRoot {
            name: name.into(),
            version: version.into(),
            volume_root: volume_root.to_str().unwrap().to_string(),
        };

        Ok(self
            .client
            .write_unit(Request::new(out))
            .await?
            .into_inner())
    }

    pub async fn remove_unit(&mut self, name: &str, version: &str) -> Result<()> {
        let out = ProtoPackageTitle {
            name: name.into(),
            version: version.into(),
        };

        Ok(self
            .client
            .remove_unit(Request::new(out))
            .await?
            .into_inner())
    }
}

impl QueryClient {
    pub async fn list_installed(&mut self) -> Result<Vec<PackageTitle>> {
        let list = self
            .client
            .list_installed(Request::new(()))
            .await?
            .into_inner();

        let mut v = Vec::new();

        for item in list.list {
            v.push(PackageTitle {
                name: item.name,
                version: item.version,
            })
        }

        Ok(v)
    }

    pub async fn list(&mut self) -> Result<Vec<PackageTitle>> {
        let list = self.client.list(Request::new(())).await?.into_inner();

        let mut v = Vec::new();

        for item in list.list {
            v.push(PackageTitle {
                name: item.name,
                version: item.version,
            })
        }

        Ok(v)
    }

    pub async fn get_responses(&mut self, name: &str) -> Result<PromptResponses> {
        let title = ProtoPackageTitle {
            name: name.into(),
            version: String::new(),
        };

        let responses = self
            .client
            .get_responses(Request::new(title))
            .await?
            .into_inner();
        let mut out = Vec::new();

        for response in responses.responses {
            out.push(response.into())
        }

        Ok(PromptResponses(out))
    }

    pub async fn get_prompts(&mut self, name: &str, version: &str) -> Result<PromptCollection> {
        let title = ProtoPackageTitle {
            name: name.into(),
            version: version.into(),
        };

        let prompts = self
            .client
            .get_prompts(Request::new(title))
            .await?
            .into_inner();

        let mut out = Vec::new();

        for prompt in &prompts.prompts {
            out.push(Prompt {
                template: prompt.template.clone(),
                question: prompt.question.clone(),
                input_type: match prompt.input_type() {
                    ProtoType::String => InputType::String,
                    ProtoType::Integer => InputType::Integer,
                    ProtoType::SignedInteger => InputType::SignedInteger,
                    ProtoType::Boolean => InputType::Boolean,
                },
            });
        }

        Ok(PromptCollection(out))
    }

    pub async fn set_responses(&mut self, name: &str, responses: PromptResponses) -> Result<()> {
        let mut out = ProtoPromptResponses {
            name: name.to_string(),
            responses: Default::default(),
        };

        for response in responses.0 {
            out.responses.push(response.into());
        }

        self.client.set_responses(Request::new(out)).await?;
        Ok(())
    }
}
