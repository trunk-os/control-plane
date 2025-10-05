mod axum_support;
mod handlers;
pub mod messages;
#[cfg(test)]
mod tests;

use self::handlers::*;
use crate::db::DB;
use crate::{config::Config, db::models::AuditLog};
use anyhow::Result;
use axum::{
	Router,
	routing::{delete, get, post, put},
};
use axum_support::WithLog;
use buckle::client::Client as BuckleClient;
use charon::Client as CharonClient;
use http::{Method, header::*};
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::{DefaultMakeSpan, DefaultOnFailure, DefaultOnRequest};
use tracing::Level;

#[derive(Debug, Clone)]
pub struct ServerState {
	buckle: BuckleClient,
	charon: CharonClient,
	db: DB,
	config: Config,
}

impl ServerState {
	pub(crate) fn with_log<T>(&self, resp: axum_support::Result<T>, log: AuditLog) -> WithLog<T> {
		WithLog(resp, log, self.clone().into())
	}
}

#[derive(Debug, Clone)]
pub struct Server {
	config: Config,
	router: Router,
}

impl Server {
	pub async fn new(config: Config) -> Result<Self> {
		Ok(Self {
			router: Router::new()
				.route("/packages/uninstall", post(uninstall_package))
				.route("/packages/install", post(install_package))
				.route("/packages/prompts", post(get_prompts))
				.route("/packages/get_responses", post(get_responses))
				.route("/packages/set_responses", post(set_responses))
				.route("/packages/installed", post(installed))
				.route("/packages/list_installed", get(list_installed))
				.route("/packages/list", get(list_packages))
				.route("/systemd/log", post(unit_log))
				.route("/systemd/list", post(list_units))
				.route("/systemd/set_unit", post(set_unit))
				.route("/status/ping", get(ping))
				.route("/status/log", post(log))
				.route("/zfs/list", post(zfs_list))
				.route("/zfs/create_volume", post(zfs_create_volume))
				.route("/zfs/create_dataset", post(zfs_create_dataset))
				.route("/zfs/modify_dataset", post(zfs_modify_dataset))
				.route("/zfs/modify_volume", post(zfs_modify_volume))
				.route("/zfs/destroy", post(zfs_destroy))
				.route("/users", put(create_user).post(list_users))
				.route(
					"/user/{id}",
					delete(remove_user)
						.get(get_user)
						.post(update_user)
						.patch(reactivate_user),
				)
				.route("/session/login", post(login))
				.route("/session/me", get(me))
				.with_state(Arc::new(ServerState {
					buckle: config.buckle()?,
					charon: config.charon()?,
					db: config.get_db().await?,
					config: config.clone(),
				}))
				.layer(
					ServiceBuilder::new()
						.layer(
							tower_http::trace::TraceLayer::new_for_http()
								.make_span_with(DefaultMakeSpan::new().level(Level::INFO))
								.on_request(DefaultOnRequest::new().level(Level::INFO))
								.on_failure(DefaultOnFailure::new().level(Level::ERROR)),
						)
						.layer(
							CorsLayer::new()
								.allow_methods([
									Method::GET,
									Method::POST,
									Method::DELETE,
									Method::PUT,
									Method::PATCH,
									Method::HEAD,
									Method::TRACE,
									Method::OPTIONS,
								])
								.allow_credentials(true)
								.allow_origin(AllowOrigin::mirror_request())
								.allow_headers([CONTENT_TYPE, ACCEPT, AUTHORIZATION])
								.allow_private_network(true),
						),
				),
			config,
		})
	}

	pub async fn start(&self) -> Result<()> {
		let handle = axum_server::Handle::new();
		tokio::spawn(shutdown_signal(handle.clone()));
		Ok(axum_server::bind(self.config.listen)
			.handle(handle)
			.serve(self.router.clone().into_make_service())
			.await?)
	}
}

async fn shutdown_signal(handle: axum_server::Handle) {
	let ctrl_c = async {
		tokio::signal::ctrl_c()
			.await
			.expect("failed to install CTRL+C signal handler");
	};

	#[cfg(unix)]
	let terminate = async {
		tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
			.expect("failed to install signal handler")
			.recv()
			.await;
	};

	tokio::select! {
		_ = ctrl_c => {},
		_ = terminate => {},
	}

	tracing::warn!("signal received, starting graceful shutdown");
	handle.graceful_shutdown(Some(std::time::Duration::from_secs(10)));
}
