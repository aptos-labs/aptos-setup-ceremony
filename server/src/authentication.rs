use common::contribution::{self, Contributor};
use common::messages::{AuthenticatedMsg, Msg};
use hyper::{Method, StatusCode};
use crate::{Request, error::{ErrorWithCode, UseCodeOnError}, config::Config};
use anyhow::{Context, Result};
use http_body_util::BodyExt;


fn verify_authenticated_by_admin<Contents: serde::Serialize>(msg: &AuthenticatedMsg<Contents>, config: &Config) -> Result<(), ErrorWithCode> {
    msg.verify_sig(&config.admin_verifying_key)
        .context("You must be authenticated as the same contributor in the message")
        .use_code_on_error(StatusCode::UNAUTHORIZED)
}

fn verify_authenticated_by_contributor<Contents: serde::Serialize>(msg: &AuthenticatedMsg<Contents>, contributor: &Contributor) -> Result<(), ErrorWithCode> {
    msg.verify_sig(&contributor.verifying_key)
        .context("You must be admin to do this")
        .use_code_on_error(StatusCode::UNAUTHORIZED)
}

pub async fn from_request(request: Request) -> Result<AuthenticatedMsg<Msg>, ErrorWithCode> {
    let method = request.method().clone();
    let uri = request.uri().clone();
    match (method, uri.path()) {
        (Method::POST, "/msg") => {
            let body = request.collect().await.unwrap().to_bytes();
            serde_json::from_slice::<AuthenticatedMsg<Msg>>(&body)
                .context("While parsing request body")
                .use_code_on_error(StatusCode::BAD_REQUEST)
        },
        _ => Err(anyhow::anyhow!("Invalid route."))
            .use_code_on_error(StatusCode::NOT_FOUND)
    }
}

pub fn verify_correctly_authenticated(msg: &AuthenticatedMsg<Msg>, config: &Config) -> Result<(), ErrorWithCode> {
    match &msg.inner {
        Msg::Join { contributor } => verify_authenticated_by_contributor(msg, contributor),
        Msg::GetStatus { contributor } => verify_authenticated_by_contributor(msg, contributor),
        Msg::UpdateDownloadProgress { contributor, .. } => verify_authenticated_by_contributor(msg, contributor),
        Msg::UpdateComputeProgress { contributor, .. } => verify_authenticated_by_contributor(msg, contributor),
        Msg::UpdateUploadProgress { contributor, .. } => verify_authenticated_by_contributor(msg, contributor),
        // admin commands
        Msg::Register { .. } => verify_authenticated_by_admin(msg, config),
        Msg::Report => verify_authenticated_by_admin(msg, config),
        Msg::DownloadAll => verify_authenticated_by_admin(msg, config),
        Msg::GetTestContributionDownloadLink { contributor } => verify_authenticated_by_contributor(msg, contributor),
        Msg::GetTestContributionUploadLink { contributor } => verify_authenticated_by_contributor(msg, contributor),
    }
}
