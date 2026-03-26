use crate::contribution::Contributor;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

const SERVER_ADDRESS : &str = "http://localhost:8888";


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
    pub async fn send<T: DeserializeOwned>(&self) -> anyhow::Result<T> {
        let client = reqwest::Client::new();
        let res = client.post(SERVER_ADDRESS)
            .json(&self)
            .send()
        .await?
        .error_for_status()?;

        Ok(serde_json::from_str(&res.text().await?)?)
    }
}

