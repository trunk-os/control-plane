use crate::{
    control_server::{Control, ControlServer},
    query_server::{Query, QueryServer},
    status_server::{Status, StatusServer},
    Config, InputType, PromptResponses, ProtoPackageInstalled, ProtoPackageTitle,
    ProtoPackageTitleList, ProtoPackageTitleWithRoot, ProtoPrompt, ProtoPromptResponses,
    ProtoPrompts, ProtoType, ResponseRegistry, SystemdUnit,
};
use std::{fs::Permissions, os::unix::fs::PermissionsExt};
use tonic::{body::Body, transport::Server as TransportServer, Result};
use tonic_middleware::{Middleware, MiddlewareLayer, ServiceBound};
use tracing::{error, info};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
pub struct Server {
    config: Config,
}

impl Server {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub fn start(
        &self,
    ) -> anyhow::Result<impl std::future::Future<Output = Result<(), tonic::transport::Error>>>
    {
        info!("Starting service.");

        if let Some(parent) = self.config.socket.to_path_buf().parent() {
            std::fs::create_dir_all(&parent)?;
        }

        if std::fs::exists(&self.config.socket)? {
            std::fs::remove_file(&self.config.socket)?;
        }

        let uds = tokio::net::UnixListener::bind(&self.config.socket)?;
        let uds_stream = tokio_stream::wrappers::UnixListenerStream::new(uds);

        std::fs::set_permissions(&self.config.socket, Permissions::from_mode(0o600))?;

        Ok(TransportServer::builder()
            .layer(MiddlewareLayer::new(LogMiddleware))
            .add_service(StatusServer::new(self.clone()))
            .add_service(ControlServer::new(self.clone()))
            .add_service(QueryServer::new(self.clone()))
            .serve_with_incoming(uds_stream))
    }
}

#[tonic::async_trait]
impl Status for Server {
    async fn ping(&self, _: tonic::Request<()>) -> Result<tonic::Response<()>> {
        Ok(tonic::Response::new(()))
    }
}

#[tonic::async_trait]
impl Control for Server {
    async fn installed(
        &self,
        title: tonic::Request<ProtoPackageTitle>,
    ) -> Result<tonic::Response<ProtoPackageInstalled>> {
        let r = self.config.registry();
        let title = title.into_inner();

        let pkg = r
            .load(&title.name, &title.version)
            .map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?
            .compile()
            .map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?;

        Ok(tonic::Response::new(ProtoPackageInstalled {
            proto_install_state: Some(
                pkg.installed()
                    .await
                    .map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?
                    .into(),
            ),
        }))
    }

    async fn install(
        &self,
        title: tonic::Request<ProtoPackageTitle>,
    ) -> Result<tonic::Response<()>> {
        let r = self.config.registry();
        let title = title.into_inner();

        let pkg = r
            .load(&title.name, &title.version)
            .map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?
            .compile()
            .map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?;

        pkg.install()
            .await
            .map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?;

        Ok(tonic::Response::new(
            self.write_unit(tonic::Request::new(ProtoPackageTitleWithRoot {
                name: title.name,
                version: title.version,
                volume_root: "/tmp/volroot".into(),
            }))
            .await?
            .into_inner(),
        ))
    }

    async fn uninstall(
        &self,
        title: tonic::Request<ProtoPackageTitle>,
    ) -> Result<tonic::Response<()>> {
        let r = self.config.registry();
        let title = title.into_inner();

        let pkg = r
            .load(&title.name, &title.version)
            .map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?
            .compile()
            .map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?;

        pkg.uninstall()
            .await
            .map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?;

        Ok(tonic::Response::new(
            self.remove_unit(tonic::Request::new(title))
                .await?
                .into_inner(),
        ))
    }

    async fn write_unit(
        &self,
        title: tonic::Request<ProtoPackageTitleWithRoot>,
    ) -> Result<tonic::Response<()>> {
        let r = self.config.registry();
        let title = title.into_inner();

        let pkg = r
            .load(&title.name, &title.version)
            .map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?
            .compile()
            .map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?;

        let unit = SystemdUnit::new(
            pkg,
            self.config.systemd_root.clone().unwrap(),
            self.config.charon_path.clone().unwrap(),
        );
        unit.create_unit(r.path(), title.volume_root.into())
            .await
            .map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?;

        info!("Wrote unit to {}", unit.filename().display());

        Ok(tonic::Response::new(()))
    }

    async fn remove_unit(
        &self,
        title: tonic::Request<ProtoPackageTitle>,
    ) -> Result<tonic::Response<()>> {
        let r = self.config.registry();
        let title = title.into_inner();

        let pkg = r
            .load(&title.name, &title.version)
            .map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?
            .compile()
            .map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?;

        let unit = SystemdUnit::new(
            pkg,
            self.config.systemd_root.clone().unwrap(),
            self.config.charon_path.clone().unwrap(),
        );
        unit.remove_unit()
            .await
            .map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?;

        info!("Removed unit {}", unit.filename().display());

        Ok(tonic::Response::new(()))
    }
}

#[tonic::async_trait]
impl Query for Server {
    async fn list_installed(
        &self,
        _empty: tonic::Request<()>,
    ) -> Result<tonic::Response<ProtoPackageTitleList>> {
        let r = self.config.registry();

        let list = r
            .installed()
            .map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?;

        let mut v = Vec::new();

        for item in list {
            v.push(ProtoPackageTitle {
                name: item.name,
                version: item.version,
            })
        }

        Ok(tonic::Response::new(ProtoPackageTitleList { list: v }))
    }

    async fn list(
        &self,
        _empty: tonic::Request<()>,
    ) -> Result<tonic::Response<ProtoPackageTitleList>> {
        let r = self.config.registry();

        let list = r
            .list()
            .map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?;

        let mut v = Vec::new();

        for item in list {
            v.push(ProtoPackageTitle {
                name: item.name,
                version: item.version,
            })
        }

        Ok(tonic::Response::new(ProtoPackageTitleList { list: v }))
    }

    async fn get_responses(
        &self,
        title: tonic::Request<ProtoPackageTitle>,
    ) -> Result<tonic::Response<ProtoPromptResponses>> {
        let r = ResponseRegistry::new(self.config.registry.path.clone());
        let title = title.into_inner();
        let responses = r
            .get(&title.name)
            .map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?;

        let mut out = ProtoPromptResponses {
            name: title.name,
            responses: Vec::with_capacity(responses.0.len()),
        };

        for response in responses.0 {
            out.responses.push(response.into());
        }

        Ok(tonic::Response::new(out))
    }

    async fn get_prompts(
        &self,
        title: tonic::Request<ProtoPackageTitle>,
    ) -> Result<tonic::Response<ProtoPrompts>> {
        let r = self.config.registry();
        let title = title.into_inner();
        let pkg = r
            .load(&title.name, &title.version)
            .map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?;
        let prompts = pkg.prompts.unwrap_or_default();

        let mut out = ProtoPrompts::default();

        for prompt in &prompts.to_vec() {
            // FIXME: do a From trait
            out.prompts.push(ProtoPrompt {
                template: prompt.template.clone(),
                question: prompt.question.clone(),
                input_type: match prompt.input_type {
                    InputType::String => ProtoType::String,
                    InputType::Integer => ProtoType::Integer,
                    InputType::SignedInteger => ProtoType::SignedInteger,
                    InputType::Boolean => ProtoType::Boolean,
                }
                .into(),
            })
        }

        Ok(tonic::Response::new(out))
    }

    async fn set_responses(
        &self,
        responses: tonic::Request<ProtoPromptResponses>,
    ) -> Result<tonic::Response<()>> {
        let r = self.config.registry();
        let responses = responses.into_inner();

        let mut pr = Vec::new();
        for response in responses.responses {
            pr.push(response.into());
        }

        r.response_registry()
            .set(&responses.name, &PromptResponses(pr))
            .map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))?;
        info!("Wrote responses for package {}", responses.name);

        Ok(tonic::Response::new(()))
    }
}

#[derive(Default, Clone)]
pub struct LogMiddleware;

#[tonic::async_trait]
impl<S> Middleware<S> for LogMiddleware
where
    S: ServiceBound,
    S::Future: Send,
    S::Error: ToString,
{
    async fn call(
        &self,
        req: http::Request<Body>,
        mut service: S,
    ) -> Result<http::Response<Body>, S::Error> {
        let uri = req.uri().clone();
        info!("GRPC Request to {}", uri.path());

        match service.call(req).await {
            Ok(x) => Ok(x),
            Err(e) => {
                error!("Error during request to {}: {}", uri.path(), e.to_string());
                Err(e)
            }
        }
    }
}
