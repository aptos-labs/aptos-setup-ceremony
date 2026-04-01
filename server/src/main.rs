use std::process;

use figment::{Figment, providers::{Env, Format, Toml}};
use server::config::Config;
use tracing::info;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _, util::SubscriberInitExt as _};

#[tokio::main]
async fn main() {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("Fatal panic: {info}");
        process::abort();
    }));

    let file_appender = tracing_appender::rolling::never("./logs", "server.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking);

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(EnvFilter::new("info"))
        .with(file_layer)
        .init();

    rustls::crypto::aws_lc_rs::default_provider().install_default()
        .expect("Failed to install rustls crypto provider");

    let config: Box<Config> = Box::new(Figment::new()
        .merge(Toml::file("config.toml"))
        .merge(Env::prefixed("SERVER_"))
        .extract()
        .expect("Failed to load config"));

    let config: &'static Config = Box::leak(config);

    info!("Starting server on port {}", &config.port);
    let (_port, server_handle) = server::serve::start_server(config)
        .await
        .expect("Failed to start server");

    server_handle.await.unwrap();
}
