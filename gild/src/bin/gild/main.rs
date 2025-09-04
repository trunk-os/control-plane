use anyhow::Result;
use gild::config::Config;
use gild::server::Server;

#[tokio::main]
async fn main() -> Result<()> {
	// FIXME: replace this with clap later
	let config = if std::env::args().len() < 2 {
		Config::default()
	} else {
		Config::from_file(
			std::env::args().skip(1).next().unwrap().into(),
		)?
	};

	Server::new(config).await?.start().await
}
