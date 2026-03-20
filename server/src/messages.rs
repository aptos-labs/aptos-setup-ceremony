use chrono::{DateTime, Utc};
use common::contribution::Contributor;
use ed25519_dalek::Signature;

pub struct Authenticator {
    
}

impl Authenticator {
    pub fn is_authorized(state: &State, msg: &Msg) -> Result<()> {
        todo!()
    }
}

pub struct AuthenticatedMsg {
    msg: Msg,
    sig: Signature,
}

pub enum Msg {
    Tick(DateTime<Utc>),
    Register {
        contributor: Contributor,
    },
    Enqueue {
        contributor: Contributor,
    },
    RequestPosition {
        contributor: Contributor,
    },
    NotifyDownloadProgress {
        progress_percent: u8,
    },
    NotifyComputeProgress {
        progress_percent: u8,
    },
    NotifyUploadProgress {
        progress_percent: u8,
    },
    UploadFailed,
    VerificationSucceeded,
    VerificationFailed,
}
