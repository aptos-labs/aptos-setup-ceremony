use common::contribution::Contributor;
use ed25519_dalek::{Signature, VerifyingKey, Verifier as _};
use hyper::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use crate::{Request, error::{ErrorWithCode, UseCodeOnError}, handlers::Config, messages::Msg};
use anyhow::{Context, Result};
use http_body_util::BodyExt;



#[derive(Serialize, Deserialize, Clone)]
pub struct AuthenticatedMsg<Contents: Serialize> {
    #[serde(flatten)]
    pub inner: Contents,
    signature: Signature,
}

impl<Contents: Serialize> AuthenticatedMsg<Contents> {
    pub fn verify_sig(&self, verifying_key: &VerifyingKey) -> Result<()> {
        Ok(verifying_key.verify(&bcs::to_bytes(&self.inner)?, &self.signature)?)
    }

    pub fn verify_authenticated_by_admin(&self, config: &Config) -> Result<(), ErrorWithCode> {
        self.verify_sig(&config.admin_verifying_key)
            .context("You must be authenticated as the same contributor in the message")
            .use_code_on_error(StatusCode::UNAUTHORIZED)
    }

    pub fn verify_authenticated_by_contributor(&self, contributor: &Contributor) -> Result<(), ErrorWithCode> {
        self.verify_sig(&contributor.verifying_key)
            .context("You must be admin to do this")
            .use_code_on_error(StatusCode::UNAUTHORIZED)
    }
}


impl AuthenticatedMsg<Msg> {
    pub async fn from_request(request: Request) -> Result<Self, ErrorWithCode> {
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

    pub fn verify_correctly_authenticated(&self, config: &Config) -> Result<(), ErrorWithCode> {
        match &self.inner {
            Msg::Join { contributor } => self.verify_authenticated_by_contributor(&contributor),
            Msg::GetStatus { contributor } => self.verify_authenticated_by_contributor(&contributor),
            Msg::UpdateDownloadProgress { contributor, .. } => self.verify_authenticated_by_contributor(&contributor),
            Msg::UpdateComputeProgress { contributor, .. } => self.verify_authenticated_by_contributor(&contributor),
            Msg::UpdateUploadProgress { contributor, .. } => self.verify_authenticated_by_contributor(&contributor),
            // admin commands
            Msg::Register { .. } => self.verify_authenticated_by_admin(config),
            Msg::Report => self.verify_authenticated_by_admin(config),
            Msg::DownloadAll => self.verify_authenticated_by_admin(config),
        }

    }
}
