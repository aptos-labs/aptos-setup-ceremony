use std::any;

use crate::contribution::Contributor;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey, Verifier as _};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use anyhow::{Context, Error, Result, bail};

const SERVER_ADDRESS : &str = "https://stannic-marguerita-detractively.ngrok-free.dev";


#[derive(Serialize, Deserialize, Clone)]
pub struct AuthenticatedMsg<Contents: Serialize> {
    #[serde(flatten)]
    pub inner: Contents,
    signature: Signature,
}

impl<Contents: Serialize> AuthenticatedMsg<Contents> {
    pub fn verify_sig(&self, verifying_key: &VerifyingKey) -> anyhow::Result<()> {
        Ok(verifying_key.verify(&bcs::to_bytes(&self.inner)?, &self.signature)?)
    }

    pub async fn send(&self) -> anyhow::Result<()> {
        let client = reqwest::Client::new();
        let res = client.post(String::from(SERVER_ADDRESS) + "/msg")
            .json(&self)
            .send()
        .await?;
        let status = res.status();
        let text = res.text().await
            .with_context(|| "While trying to fetch response text")?;
        if status.is_client_error() || status.is_server_error() {
            anyhow::bail!("Server returned an error: {}, with body: {}", status, text)
        } else {
            Ok(())
        }
    }

    pub async fn send_and_receive<T: DeserializeOwned>(&self) -> anyhow::Result<T> {
        let client = reqwest::Client::new();
        let res = client.post(String::from(SERVER_ADDRESS) + "/msg")
            .json(&self)
            .send()
        .await?;
        let status = res.status();
        let text = res.text().await
            .with_context(|| "While trying to fetch response text")?;
        if status.is_client_error() || status.is_server_error() {
            anyhow::bail!("Server returned an error: {}, with body: {}", status, text)
        } else {
            Ok(serde_json::from_str(&text)?)
        }

    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum Msg {
    Report,
    DownloadAll,
    Register {
        contributor: Contributor,
    },
    Join {
        contributor: Contributor,
    },
    GetStatus {
        contributor: Contributor,
    },
    UpdateDownloadProgress {
        finished: bool,
        contributor: Contributor,
    },
    UpdateComputeProgress {
        finished: bool,
        contributor: Contributor,
    },
    UpdateUploadProgress {
        finished: bool,
        contributor: Contributor,
    },
}

impl Msg {
    pub fn sign(self, sk: &SigningKey) -> AuthenticatedMsg<Self> {
        let signature = sk.sign(&bcs::to_bytes(&self).expect("BCS serialization of Msg should never fail"));
        AuthenticatedMsg {
            inner: self,
            signature,
        }
    }
}

