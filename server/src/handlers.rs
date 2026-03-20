use common::contribution::Contributor;
use ed25519_dalek::VerifyingKey;
use anyhow::{Context, Result};

use crate::{authentication::AuthenticatedMsg, store::{contribution_files::{ContributionFileHandle, ContributionFilesStore}, contributors::ContributorsDB}};
use crate::store::contributors::ContributorStatus;



pub struct Config {
    pub bucket_id: String,
    pub admin_verifying_key: VerifyingKey,
}

pub struct State {
    pub contributors_db: ContributorsDB,
    pub contribution_files_store: ContributionFilesStore,
}

pub enum StatusResponse {
    DidntJoinOrKicked,
    WaitingInQueue(usize),
    ReadyToDownloadPrevious(ContributionFileHandle),
    WaitingForContributionWithPrevious(ContributionFileHandle),
    ReadyForUpload(ContributionFileHandle),
    Verifying,
    Finished,
    Stopped,
}

pub async fn handle_get_status(c: &AuthenticatedMsg<Contributor>, state: &mut State, _config: &Config) -> Result<StatusResponse> {
    c.verify_authenticated_by_contributor()?;

    Ok(match state.contributors_db.get_contributor_status(&c.inner).await? {
        ContributorStatus::DidntJoinQueue => StatusResponse::DidntJoinOrKicked,
        ContributorStatus::Queued { joined: _, pos } => {
            if pos > 0 {
                StatusResponse::WaitingInQueue(pos)
            } else {
                match state.contributors_db.get_global_status().await? {
                    crate::store::contributors::Status::WaitingForDownload => 
                    StatusResponse::ReadyToDownloadPrevious(
                        state.contribution_files_store.get_or_create(
                            &state.contributors_db.get_most_recent_finished_contributor().await?
                        ).await?
                            .should_be_finished()
                            .context("While constructing URL for downloading previous finished contribution")?
                    ),
                    crate::store::contributors::Status::WaitingForCompute => 
                    StatusResponse::WaitingForContributionWithPrevious(
                        state.contribution_files_store.get_or_create(
                            &state.contributors_db.get_most_recent_finished_contributor().await?
                        ).await?
                            .should_be_finished()
                            .context("While constructing URL for downloading previous finished contribution")?
                    ),
                    crate::store::contributors::Status::WaitingForUpload => 
                    StatusResponse::ReadyForUpload(
                        state.contribution_files_store.get_or_create(&c.inner).await?
                            .should_not_be_finished()
                            .context("While constructing URL for uploading current contribution")?
                    ),
                    crate::store::contributors::Status::Verifying => 
                    StatusResponse::Verifying,
                    crate::store::contributors::Status::Stopped => 
                    StatusResponse::Stopped,
                }
            }
        },
        ContributorStatus::Kicked => StatusResponse::DidntJoinOrKicked,
        ContributorStatus::Finished { .. } => StatusResponse::Finished,
    })
}
