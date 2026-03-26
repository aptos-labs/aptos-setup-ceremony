use common::contribution::Contributor;
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};



#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum Msg {
    Report,
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


