use tonic::{
	Result,
	body::Body,
	codegen::http::{Request, Response},
};
use tonic_middleware::{Middleware, ServiceBound};
use tracing::{error, info};

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
		&self, req: Request<Body>, mut service: S,
	) -> Result<Response<Body>, S::Error> {
		let uri = req.uri().clone();
		info!("GRPC Request to {}", uri.path());

		match service.call(req).await {
			Ok(x) => Ok(x),
			Err(e) => {
				error!(
					"Error during request to {}: {}",
					uri.path(),
					e.to_string()
				);
				Err(e)
			}
		}
	}
}
