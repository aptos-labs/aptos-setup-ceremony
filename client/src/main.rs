use std::process;
use clap::Parser;

use client::{cli::Cli, run};
use tracing::{error, level_filters::LevelFilter};
use tracing_subscriber::{Layer as _, layer::SubscriberExt as _, util::SubscriberInitExt as _};


#[tokio::main]
async fn main() {
     std::panic::set_hook(Box::new(|info| {
        eprintln!("Fatal panic: {info}");
        process::abort();
    }));

    let file_appender = tracing_appender::rolling::never("./", "contribution.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_filter(LevelFilter::INFO);

    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(LevelFilter::WARN);

    tracing_subscriber::registry()
        .with(file_layer)
        .with(stderr_layer)
        .init();


    let cli = Cli::parse();
    let config_dir = dirs::config_dir()
        .map(|d| d.join("aptos-setup-ceremony"))
        .expect("no config dir found");
        
    std::fs::create_dir_all(&config_dir).expect("Should be able to create dir");

    if let Err(e) = run::run(cli, config_dir).await {
        error!("{:?}", e);
    }

}
