pub mod verification_job;
pub mod store;
pub mod authentication;
pub mod handlers;
pub mod messages;
pub mod error;

use std::sync::Arc;

use authentication::AuthenticatedMsg;
use error::ErrorWithCode;
use figment::{Figment, providers::{Env, Format, Toml}};
use handlers::{Config, State, handle};
use hyper::{Response, body::Bytes};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use http_body_util::Full;
use common::messages::Msg;
use store::{contribution_files::ContributionFilesStore, contributors_db::ContributorsDB};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

pub type Request = hyper::Request<hyper::body::Incoming>;

async fn request_handler(
    request: Request,
    state: Arc<Mutex<State>>,
    config: Arc<Config>,
) -> Result<Response<Full<Bytes>>, std::convert::Infallible> {
    let result = handle_request(request, state, config).await;
    Ok(match result {
        Ok(value) => {
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
            Response::builder()
                .status(code)
                .body(Full::new(Bytes::from(body)))
                .unwrap()
        }
    })
}

async fn handle_request(
    request: Request,
    state: Arc<Mutex<State>>,
    config: Arc<Config>,
) -> Result<serde_json::Value, ErrorWithCode> {
    let authenticated_msg: AuthenticatedMsg<Msg> = AuthenticatedMsg::from_request(request).await?;
    authenticated_msg.verify_correctly_authenticated(&config)?;
    let mut state = state.lock().await;
    handle(authenticated_msg.inner, &mut state, &config).await
}

#[tokio::main]
async fn main() {
    let config: Config = Figment::new()
        .merge(Toml::file("config.toml"))
        .merge(Env::prefixed("SERVER_"))
        .extract()
        .expect("Failed to load config");

    let contributors_db = ContributorsDB::new(&config.db_path)
        .await
        .expect("Failed to initialize database");

    let contribution_files_store = ContributionFilesStore::init(&config.gcp_project_id, &config.bucket_id)
        .await
        .expect("Failed to initialize contribution files store");

    let addr = format!("0.0.0.0:{}", config.port);
    let config = Arc::new(config);
    let state = Arc::new(Mutex::new(State {
        contributors_db,
        contribution_files_store,
        current_verification_job: None,
    }));

    let listener = TcpListener::bind(&addr).await.unwrap();
    eprintln!("Listening on {addr}");
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
                eprintln!("Error serving connection: {e:?}");
            }
        });
    }
}
