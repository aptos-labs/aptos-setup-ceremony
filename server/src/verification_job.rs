use std::time::Instant;

use bytes::Bytes;
use common::{aptos::AptosParams, constants::CeremonyContribution, contribution::Contributor, errors::ContributionVerificationFailure};
use rand::thread_rng;
use rayon;
use anyhow::{Context, Result};
use tokio::sync::oneshot;

use crate::store::contribution_files::ContributionFilesStore;


pub struct VerificationJob {
    pub rx: oneshot::Receiver<Result<(), ContributionVerificationFailure>>
}

impl VerificationJob {
    pub async fn start(
        current: &Contributor, 
        hash: String,
        maybe_previous: &Option<Contributor>, 
        contribution_files_store: &ContributionFilesStore,
        params: &AptosParams,
    ) -> Result<Self> {
        let current_contribution_bytes : Bytes = contribution_files_store.download_contribution(&current).await?;

        let start = Instant::now();
        let current_contribution_hash = blake3::hash(&current_contribution_bytes);
        println!("time taken to hash: {:?}", start.elapsed());

        if current_contribution_hash.to_string() != hash {
            anyhow::bail!("Contribution hash mismatch");
        }

        let current_contribution : CeremonyContribution = bcs::from_bytes(&current_contribution_bytes)
        .context("Error while parsing the uploaded contribution")?;
        let maybe_previous_contribution : Option<CeremonyContribution> = match maybe_previous {
            Some(previous) => Some(bcs::from_bytes(&contribution_files_store.download_contribution(&previous).await?)
                .context("Error while parsing the previous contribution")?),
            None => None
        };

        let (tx, rx) = oneshot::channel();
        let params = params.clone();


        rayon::spawn(move || {
            let mut rng = thread_rng();
            let verification_result = current_contribution.verify(&mut rng, maybe_previous_contribution.as_ref(), &params);
            tx.send(verification_result)
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
