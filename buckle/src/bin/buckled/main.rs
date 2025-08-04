use buckle::{config::Config, server::Server};

#[tokio::main]
pub async fn main() -> Result<(), anyhow::Error> {
    let config = if std::env::args().len() != 1 {
        Config::from_file(std::env::args().skip(1).next().unwrap().into())?
    } else {
        Config::default()
    };

    if let Err(e) = Server::new_with_config(Some(config)).start()?.await {
        tracing::error!("Error while running service: {}", e.to_string());
        return Err(e.into());
    }

    Ok(())
}
