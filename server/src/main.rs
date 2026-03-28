use std::process;
use std::sync::Arc;

use figment::{Figment, providers::{Env, Format, Toml}};
use server::config::Config;
use tracing::info;

#[tokio::main]
async fn main() {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("Fatal panic: {info}");
        process::abort();
    }));

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();

    rustls::crypto::aws_lc_rs::default_provider().install_default()
        .expect("Failed to install rustls crypto provider");

    let config: Config = Figment::new()
        .merge(Toml::file("config.toml"))
        .merge(Env::prefixed("SERVER_"))
        .extract()
        .expect("Failed to load config");

    info!("Starting server on port {}", config.port);
    let (_port, server_handle) = server::serve::start_server(Arc::new(config))
        .await
        .expect("Failed to start server");

    server_handle.await.unwrap();
}
