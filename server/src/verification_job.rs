use common::contribution::Contributor;
use rayon;
use anyhow::Result;

use crate::store::contribution_files::{self, ContributionFilesStore};

pub struct VerificationJob {
    
}

impl VerificationJob {
    pub fn start(contributor: &Contributor, previous: &Contributor, contribution_files_store: &ContributionFilesStore) -> Self {
        rayon::spawn(|| {});
        todo!()
    }

    pub async fn finished(&self ) -> Result<()> {
        rayon::spawn(|| {});
        todo!()
    }
}
