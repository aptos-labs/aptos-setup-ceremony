use common::contribution::Contributor;
use rayon;
use anyhow::Result;


pub struct VerificationJob {
    
}

impl VerificationJob {
    pub fn start(_contributor: &Contributor) -> Self { //, previous: &Contributor, contribution_files_store: &ContributionFilesStore) -> Self {
        rayon::spawn(|| {});
        todo!()
    }

    pub async fn finished(&self ) -> Result<()> {
        rayon::spawn(|| {});
        todo!()
    }
}
