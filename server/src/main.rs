use std::{process, sync::Arc};

use server::{error::ErrorWithCode, handlers::handle_tick};
use figment::{Figment, providers::{Env, Format, Toml}};
use server::handlers::{Config, State, handle};
use hyper::{Response, body::Bytes};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use http_body_util::Full;
use server::store::{contribution_files::ContributionFilesStore, contributors_db::ContributorsDB};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tracing::{info, warn, error, debug};


async fn request_handler(
    request: server::Request,
    state: Arc<Mutex<State>>,
    config: Arc<Config>,
) -> Result<Response<Full<Bytes>>, std::convert::Infallible> {
    let method = request.method().clone();
    let uri = request.uri().clone();
    let result = handle_request(request, state, config).await;
    Ok(match result {
        Ok(value) => {
            info!(%method, %uri, status = 200, "request handled");
            let body = serde_json::to_string(&value).unwrap();
            Response::builder()
                .status(200)
                .header("Content-Type", "application/json")
                .body(Full::new(Bytes::from(body)))
                .unwrap()
        }
        Err(err) => {
            let code = err.code();
            let body = format!("{:#}", err.error);
            if code.is_server_error() {
                error!(%method, %uri, status = code.as_u16(), error = %body, "request failed");
            } else {
                warn!(%method, %uri, status = code.as_u16(), error = %body, "request rejected");
            }
            Response::builder()
                .status(code)
                .body(Full::new(Bytes::from(body)))
                .unwrap()
        }
    })
}

async fn handle_request(
    request: server::Request,
    state: Arc<Mutex<State>>,
    config: Arc<Config>,
) -> Result<serde_json::Value, ErrorWithCode> {
    let authenticated_msg = server::authentication::from_request(request).await?;
    debug!(msg = ?authenticated_msg.inner, "authenticated request");
    server::authentication::verify_correctly_authenticated(&authenticated_msg, &config)?;
    let mut state = state.lock().await;
    handle(authenticated_msg.inner, &mut state, &config).await
}

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

    info!("Initializing database at {}", config.db_path);
    let contributors_db = ContributorsDB::new(&config.db_path)
        .await
        .expect("Failed to initialize database");

    info!("Initializing GCS store (project={}, bucket={})", config.gcp_project_id, config.bucket_id);
    let contribution_files_store = ContributionFilesStore::init(&config.gcp_project_id, &config.bucket_id)
        .await
        .expect("Failed to initialize contribution files store");

    contribution_files_store.ensure_bucket_exists().await
        .expect("Failed to ensure GCS bucket exists");

    let addr = format!("0.0.0.0:{}", config.port);
    let config = Arc::new(config);
    let state = Arc::new(Mutex::new(State {
        contributors_db,
        contribution_files_store,
    }));

    let state_cloned = state.clone();
    let config_cloned = config.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(tokio::time::Duration::from_secs(5));
        loop {
            ticker.tick().await;
            let mut state_locked = state_cloned.lock().await;
            handle_tick(&mut state_locked, &config_cloned).await
            .expect("Should never fail to tick");
        }
    });

    let listener = TcpListener::bind(&addr).await.unwrap();
    info!("Listening on {addr}");
    loop {
        let (stream, _) = listener.accept().await.unwrap();
        let io = TokioIo::new(stream);
        let state = state.clone();
        let config = config.clone();
        tokio::task::spawn(async move {
            if let Err(e) = http1::Builder::new()
                .serve_connection(io, service_fn(|req| request_handler(req, state.clone(), config.clone())))
                .await
            {
                error!("Error serving connection: {e:?}");
            }
        });
    }
}
