use crate::contribution::Contributor;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey, Verifier as _};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

const SERVER_ADDRESS : &str = "http://localhost:8888";


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
        client.post(SERVER_ADDRESS)
            .json(&self)
            .send()
        .await?
        .error_for_status()?;

        Ok(())
    }

    pub async fn send_and_receive<T: DeserializeOwned>(&self) -> anyhow::Result<T> {
        let client = reqwest::Client::new();
        let res = client.post(SERVER_ADDRESS)
            .json(&self)
            .send()
        .await?
        .error_for_status()?;

        Ok(serde_json::from_str(&res.text().await?)?)
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

