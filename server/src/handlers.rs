use common::contribution::Contributor;
use ed25519_dalek::VerifyingKey;
use anyhow::{Context, Result, bail};

use crate::{authentication::AuthenticatedMsg, store::{contribution_files::{ContributionFileHandle, ContributionFilesStore}, contributors::{ContributorsDB, Status}}, verification_job::VerificationJob};
use crate::store::contributors::ContributorStatus;



pub struct Config {
    pub bucket_id: String,
    pub admin_verifying_key: VerifyingKey,
}

pub struct State {
    pub contributors_db: ContributorsDB,
    pub contribution_files_store: ContributionFilesStore,
    pub current_verification_job: Option<VerificationJob>,
}

pub enum StatusResponse {
    DidntJoin,
    Kicked(anyhow::Error),
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
        ContributorStatus::DidntJoinQueue => StatusResponse::DidntJoin,
        ContributorStatus::Queued { joined: _, pos } => {
            if pos > 0 {
                StatusResponse::WaitingInQueue(pos)
            } else {
                match state.contributors_db.get_global_status().await? {
                    crate::store::contributors::Status::WaitingForDownload(_) => 
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
                    crate::store::contributors::Status::WaitingForUpload(_) => 
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
        ContributorStatus::Kicked { err, .. } => StatusResponse::Kicked(err),
        ContributorStatus::Finished { .. } => StatusResponse::Finished,
    })
}

pub async fn handle_update_download_progress(progress_percent: u8, c: AuthenticatedMsg<Contributor>, state: &mut State, _config: &Config) -> Result<()> {
    c.verify_authenticated_by_contributor()?;

    let ContributorStatus::Queued { joined: _, pos: 0 } = state.contributors_db.get_contributor_status(&c.inner).await? else {
        bail!("Not the current active contributor");
    };
    let Status::WaitingForDownload(_) =  state.contributors_db.get_global_status().await? else {
        bail!("Not currently downloading");
    };

    if progress_percent < 100 {
        state.contributors_db.set_global_status(Status::WaitingForDownload(progress_percent)).await?;
    } else {
        state.contributors_db.set_global_status(Status::WaitingForCompute).await?;
    }
    state.contributors_db.update_timestamp(&c.inner).await?;

    Ok(())
}

pub async fn handle_update_compute_progress(finished: bool, c: AuthenticatedMsg<Contributor>, state: &mut State, _config: &Config) -> Result<()> {
    c.verify_authenticated_by_contributor()?;

    let ContributorStatus::Queued { joined: _, pos: 0 } = state.contributors_db.get_contributor_status(&c.inner).await? else {
        bail!("Not the current active contributor");
    };
    let Status::WaitingForCompute = state.contributors_db.get_global_status().await? else {
        bail!("Not currently waiting for compute");
    };

    if finished {
        state.contributors_db.set_global_status(Status::WaitingForUpload(0)).await?;
    }
    state.contributors_db.update_timestamp(&c.inner).await?;

    Ok(())
}

pub async fn handle_update_upload_progress(progress_percent: u8, c: AuthenticatedMsg<Contributor>, state: &mut State, _config: &Config) -> Result<()> {
    c.verify_authenticated_by_contributor()?;

    let ContributorStatus::Queued { joined: _, pos: 0 } = state.contributors_db.get_contributor_status(&c.inner).await? else {
        bail!("Not the current active contributor");
    };
    let Status::WaitingForCompute = state.contributors_db.get_global_status().await? else {
        bail!("Not currently waiting for compute");
    };

    if progress_percent < 100 {
        state.contributors_db.set_global_status(Status::WaitingForUpload(progress_percent)).await?;
        state.contributors_db.update_timestamp(&c.inner).await?;
    } else {
        state.contributors_db.set_global_status(Status::Verifying).await?;
        state.contributors_db.update_timestamp(&c.inner).await?;
        state.current_verification_job = Some(VerificationJob::start(&c.inner)); 
        match state.current_verification_job.unwrap().finished().await {
            Ok(_) => {
                state.contributors_db.finish_current().await?;
            },
            Err(e) => {
                state.contributors_db.kick_current(e).await?;
            }
        }
    }

    Ok(())
}



