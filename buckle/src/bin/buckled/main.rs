use buckle::{
	config::Config,
	migration::{plans::migrations, run_migrations},
	server::Server,
};

#[tokio::main]
pub async fn main() -> Result<(), anyhow::Error> {
	let config = if std::env::args().len() != 1 {
		match std::env::args().nth(1).unwrap().as_str() {
			"migrate" => {
				print!("running migrations...");
				if let Err(e) = run_migrations(migrations(), Default::default()).await {
					println!("error: {}", e);
				}
				println!("done.");
				std::process::exit(0);

				// NOTE: this just keeps the match clean. This code still doesn't run.
				#[allow(unreachable_code)]
				Config::default()
			}
			x => Config::from_file(x.into())?,
		}
	} else {
		Config::default()
	};

	if let Err(e) = run_migrations(migrations(), Default::default()).await {
		tracing::error!("Error running migrations: {}", e);
	}

	if let Err(e) = Server::new_with_config(Some(config)).start()?.await {
		tracing::error!("Error while running service: {}", e.to_string());
		return Err(e.into());
	}

	Ok(())
}
