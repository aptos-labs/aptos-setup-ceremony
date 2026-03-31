use std::sync::Arc;

use anyhow::Result;
use common::constants::TEST_DOWNLOAD_BLOB_SIZE_BYTES;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Response, body::Bytes};
use hyper_util::rt::TokioIo;
use http_body_util::Full;
use rand::{Rng, thread_rng};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{info, warn, error, debug};

use crate::error::ErrorWithCode;
use crate::handlers::{State, handle, handle_tick};
use crate::config::Config;
use crate::store::contribution_files::ContributionFilesStore;
use crate::store::contributors_db::ContributorsDB;
use crate::Request;

async fn request_handler(
    request: Request,
    state: Arc<State>,
    config: Arc<Config>,
) -> Result<Response<Full<Bytes>>, std::convert::Infallible> {
    let method = request.method().clone();
    let uri = request.uri().clone();
    let result = deserialize_authenticate_and_handle(request, state, config).await;
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
            let error_desc_full = format!("{:?}", err.error);
            let body = format!("{}", err.error);
            if code.is_server_error() {
                error!(%method, %uri, status = code.as_u16(), error = %error_desc_full, "request failed");
            } else {
                warn!(%method, %uri, status = code.as_u16(), error = %error_desc_full, "request rejected");
            }
            Response::builder()
                .status(code)
                .body(Full::new(Bytes::from(body)))
                .unwrap()
        }
    })
}

async fn deserialize_authenticate_and_handle(
    request: Request,
    state: Arc<State>,
    config: Arc<Config>,
) -> Result<serde_json::Value, ErrorWithCode> {
    let authenticated_msg = crate::authentication::from_request(request).await?;
    debug!(msg = ?authenticated_msg.inner, "authenticated request");
    crate::authentication::verify_correctly_authenticated(&authenticated_msg, &config)?;
    handle(authenticated_msg.inner, state, &config).await
}

/// Initializes the database, GCS store, tick loop, and HTTP listener.
/// Returns `(port, server_join_handle)`.
pub async fn start_server(config: Arc<Config>) -> Result<(u16, JoinHandle<()>)> {
    info!("Initializing database at {}", config.db_path);
    let contributors_db = ContributorsDB::new(&config.db_path).await?;


    // note: looks in GOOGLE_APPLICATION_CREDENTIALS for json cred file
    info!(
        "Initializing GCS store (project={}, bucket={})",
        config.gcp_project_id, config.bucket_id
    );
    let contribution_files_store =
        ContributionFilesStore::init(&config.gcp_project_id, &config.bucket_id).await?;
    contribution_files_store.ensure_bucket_exists().await?;


    info!(
        "Generating test blob for client download tests"
    );
    let mut rng = thread_rng();
    let mut test_download_blob = vec![0; TEST_DOWNLOAD_BLOB_SIZE_BYTES];
    rng.fill(&mut test_download_blob[..]);
    contribution_files_store.write_test_blob(Bytes::from(test_download_blob)).await?;


    let state = Arc::new(State {
        contributors_db: Arc::new(Mutex::new(contributors_db)),
        contribution_files_store,
    });

    // Tick loop
    let tick_state = state.clone();
    let tick_config = config.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(tokio::time::Duration::from_secs(5));
        loop {
            ticker.tick().await;
            // TODO why do I have to clone twice here??
            if let Err(e) = handle_tick(tick_state.clone(), &tick_config).await {
                error!("Tick error: {e:?}");
            }
        }
    });

    // HTTP listener
    let addr = format!("0.0.0.0:{}", config.port);
    let listener = TcpListener::bind(&addr).await?;
    let port = listener.local_addr()?.port();
    info!("Listening on 0.0.0.0:{port}");

    let server_handle = tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let io = TokioIo::new(stream);
            let state = state.clone();
            let config = config.clone();
            tokio::task::spawn(async move {
                if let Err(e) = http1::Builder::new()
                    .serve_connection(
                        io,
                        service_fn(|req| {
                            let state = state.clone();
                            let config = config.clone();
                            async move { request_handler(req, state, config).await }
                        }),
                    )
                    .await
                {
                    error!("Error serving connection: {e:?}");
                }
            });
        }
    });

    Ok((port, server_handle))
}
