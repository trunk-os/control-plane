use charon::{Config, Server};

#[tokio::main]
pub async fn main() -> Result<(), anyhow::Error> {
	let config = Config::from_file(
		std::env::args()
			.nth(1)
			.expect("Expected a config file")
			.into(),
	)?;

	if let Err(e) = Server::new(config).start()?.await {
		tracing::error!("Error while running service: {}", e.to_string());
		return Err(e.into());
	}

	Ok(())
}
