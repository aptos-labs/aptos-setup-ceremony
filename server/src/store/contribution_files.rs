use common::contribution::Contributor;
use anyhow::{Result, bail};



/// Represents a contribution file in the GCS
pub enum ContributionFileHandle {
    InProgress {
        contributor: Contributor,
        upload_session_url: String,
    },
    Complete {
        contributor: Contributor,
    }
}

impl ContributionFileHandle {
    pub fn url(&self, store: &ContributionFilesStore) -> String {
        todo!("Should be a deterministic url based on the contributor and bucket")
    }

    pub fn should_be_finished(self) -> Result<Self> {
        match self {
            ContributionFileHandle::InProgress { .. } => 
            bail!("Expected contribution file to be finished, but got in progress"),
            ContributionFileHandle::Complete { .. } => Ok(self),
        }
    }

    pub fn should_not_be_finished(self) -> Result<Self> {
        match self {
            ContributionFileHandle::InProgress { .. } => Ok(self),
            ContributionFileHandle::Complete { .. } => 
            bail!("Expected contribution file to be in progress, but got a finished contribution")
        }
    }
}


pub struct ContributionFilesStore {
}

impl ContributionFilesStore {
    pub fn init(
        bucket_id: &String, 
        // whatever is needed for auth with GCS
    ) -> Self {
        todo!("Initialize with auth and bucked it (not sure if ID should be a string or something else)")
    }

    pub async fn get_bucket_id(&self) -> String {
        todo!()
    }

    pub async fn get_or_create(&mut self, c: &Contributor) -> Result<ContributionFileHandle> {
        todo!("If doesn't exist, should start a new upload in GCS, get the session_url, and return an InProgress file handle with that url")
    }

}
