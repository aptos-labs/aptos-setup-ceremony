use common::{contribution::{Contribution, Contributor}, errors::ContributionVerificationFailure, fptx::{FPTXContributionInner, FPTXParams}};
use rand::thread_rng;
use rayon;
use anyhow::Result;
use tokio::sync::oneshot;

use crate::store::contribution_files::ContributionFilesStore;


pub struct VerificationJob {
    pub rx: oneshot::Receiver<Result<(), ContributionVerificationFailure>>
}

impl VerificationJob {
    pub async fn start(
        current: &Contributor, 
        maybe_previous: &Option<Contributor>, 
        contribution_files_store: &ContributionFilesStore,
        params: &FPTXParams,
    ) -> Result<Self> {
        let current_contribution : Contribution<FPTXContributionInner> = contribution_files_store.download_contribution(&current).await?;
        let maybe_previous_contribution : Option<Contribution<FPTXContributionInner>> = match maybe_previous {
            Some(previous) => Some(contribution_files_store.download_contribution(&previous).await?),
            None => None
        };

        let (tx, rx) = oneshot::channel();
        let params = params.clone();

        rayon::spawn(move || {
            let mut rng = thread_rng();
            tx.send(current_contribution.verify(&mut rng, maybe_previous_contribution.as_ref(), &params))
            .expect("Sending should always succeed")
        });

        Ok(Self {
            rx
        })
    }

    pub async fn finished(self) -> Result<Result<(), ContributionVerificationFailure>> {
        Ok(self.rx.await?)
    }
}
