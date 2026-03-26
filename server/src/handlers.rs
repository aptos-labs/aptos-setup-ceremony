use chrono::{TimeDelta, Utc};
use common::contribution::Contributor;
use ed25519_dalek::VerifyingKey;
use anyhow::{Context, Result, bail};

use crate::{authentication::AuthenticatedMsg, store::{contribution_files::{ContributionFileHandle, ContributionFilesStore}, contributors_db::{ContributorState, ContributorsDB, Status}}, verification_job::VerificationJob};
use crate::store::contributors_db::ContributorStatus;

const PING_TIMEOUT : TimeDelta = TimeDelta::seconds(20);
const DOWNLOAD_TIMEOUT : TimeDelta = TimeDelta::seconds(60);
const COMPUTE_TIMEOUT : TimeDelta = TimeDelta::seconds(720);
const UPLOAD_TIMEOUT : TimeDelta = TimeDelta::seconds(120);


pub struct Config {
    pub bucket_id: String,
    pub admin_verifying_key: VerifyingKey,
    pub ping_timeout: TimeDelta,
    pub download_timeout: TimeDelta,
    pub contribute_timeout: TimeDelta,
    pub upload_timeout: TimeDelta,
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
}


pub async fn lookup(key: &VerifyingKey, state: &mut State, _config: &Config) -> Result<()> {
    match state.contributors_db.get_contributor_status(&c).await? {
        ContributorStatus::DidntJoinQueue 
        | ContributorStatus::Kicked {..} => {
            state.contributors_db.enqueue(&c).await
        },
        ContributorStatus::Queued {..} => {
            bail!("Already in queue")
        }
        ContributorStatus::Finished {  } => {
            bail!("Already finished contributing")
        }
    }
}

pub async fn handle_join(c: &Contributor, state: &mut State, _config: &Config) -> Result<()> {
    match state.contributors_db.get_contributor_status(&c).await? {
        ContributorStatus::DidntJoinQueue 
        | ContributorStatus::Kicked {..} => {
            state.contributors_db.enqueue(&c).await
        },
        ContributorStatus::Queued {..} => {
            bail!("Already in queue")
        }
        ContributorStatus::Finished {  } => {
            bail!("Already finished contributing")
        }
    }
}

pub async fn handle_get_status(c: &Contributor, state: &mut State, _config: &Config) -> Result<StatusResponse> {
    Ok(match state.contributors_db.get_contributor_status(&c).await? {
        ContributorStatus::DidntJoinQueue => StatusResponse::DidntJoin,
        ContributorStatus::Queued { joined: _, pos } => {
            if pos > 0 {
                StatusResponse::WaitingInQueue(pos)
            } else {
                match state.contributors_db.get_global_status().await? {
                    crate::store::contributors_db::Status::WaitingForDownload {..} => 
                    StatusResponse::ReadyToDownloadPrevious(
                        state.contribution_files_store.get_or_create(
                            &state.contributors_db.get_most_recent_finished_contributor().await?
                        ).await?
                            .should_be_finished()
                            .context("While constructing URL for downloading previous finished contribution")?
                    ),
                    crate::store::contributors_db::Status::WaitingForCompute {..} => 
                    StatusResponse::WaitingForContributionWithPrevious(
                        state.contribution_files_store.get_or_create(
                            &state.contributors_db.get_most_recent_finished_contributor().await?
                        ).await?
                            .should_be_finished()
                            .context("While constructing URL for downloading previous finished contribution")?
                    ),
                    crate::store::contributors_db::Status::WaitingForUpload {..} => 
                    StatusResponse::ReadyForUpload(
                        state.contribution_files_store.get_or_create(&c).await?
                            .should_not_be_finished()
                            .context("While constructing URL for uploading current contribution")?
                    ),
                    crate::store::contributors_db::Status::Verifying { .. } => 
                    StatusResponse::Verifying,
                }
            }
        },
        ContributorStatus::Kicked { err, .. } => StatusResponse::Kicked(err),
        ContributorStatus::Finished { .. } => StatusResponse::Finished,
    })
}

pub async fn handle_update_download_progress(finished: bool, c: Contributor, state: &mut State, _config: &Config) -> Result<()> {
    let ContributorStatus::Queued { joined: _, pos: 0 } = state.contributors_db.get_contributor_status(&c).await? else {
        bail!("Not the current active contributor");
    };
    let Status::WaitingForDownload{..} =  state.contributors_db.get_global_status().await? else {
        bail!("Not currently downloading");
    };

    if finished {
        state.contributors_db.set_global_status(Status::WaitingForCompute { start: Utc::now() } ).await?;
    }
    state.contributors_db.update_timestamp(&c).await?;

    Ok(())
}

pub async fn handle_update_compute_progress(finished: bool, c: Contributor, state: &mut State, _config: &Config) -> Result<()> {
    let ContributorStatus::Queued { joined: _, pos: 0 } = state.contributors_db.get_contributor_status(&c).await? else {
        bail!("Not the current active contributor");
    };
    let Status::WaitingForCompute { .. } = state.contributors_db.get_global_status().await? else {
        bail!("Not currently waiting for compute");
    };

    if finished {
        state.contributors_db.set_global_status(Status::WaitingForUpload { start: Utc::now() }).await?;
    }
    state.contributors_db.update_timestamp(&c).await?;

    Ok(())
}

pub async fn handle_update_upload_progress(finished: bool, c: Contributor, state: &mut State, _config: &Config) -> Result<()> {
    let ContributorStatus::Queued { joined: _, pos: 0 } = state.contributors_db.get_contributor_status(&c).await? else {
        bail!("Not the current active contributor");
    };
    let Status::WaitingForUpload { .. } = state.contributors_db.get_global_status().await? else {
        bail!("Not currently waiting for upload");
    };

    state.contributors_db.update_timestamp(&c).await?;
    if finished {
        state.contributors_db.set_global_status(Status::Verifying { start: Utc::now() }).await?;
        state.current_verification_job = Some(VerificationJob::start(&c)); 
        match state.current_verification_job.as_ref().unwrap().finished().await {
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


pub async fn handle_tick(state: &mut State, _config: &Config) -> Result<()> {
    let current_contributor = state.contributors_db.get_current().await?;
    let status = state.contributors_db.get_global_status().await?;

    let current_time = Utc::now();

    if current_time - current_contributor.updated_timestamp > PING_TIMEOUT {
        state.contributors_db.kick_current(anyhow::anyhow!("Timed out")).await?;
        return Ok(());
    } 
    match status {
        Status::WaitingForDownload { start } => {
            if current_time - start > DOWNLOAD_TIMEOUT {
                state.contributors_db.kick_current(anyhow::anyhow!("Timed out")).await?;
            }
        }
        Status::WaitingForCompute { start } => {
            if current_time - start > COMPUTE_TIMEOUT {
                state.contributors_db.kick_current(anyhow::anyhow!("Timed out")).await?;
            }
        }
        Status::WaitingForUpload { start } => {
            if current_time - start > UPLOAD_TIMEOUT {
                state.contributors_db.kick_current(anyhow::anyhow!("Timed out")).await?;
            }
        }
        Status::Verifying { .. } => (),
    }

    Ok(())
}


pub async fn handle_register(c: &Contributor, state: &mut State, _config: &Config) -> Result<()> {
    state.contributors_db.register(&c).await?;
    Ok(())
}

pub async fn handle_report(state: &mut State, _config: &Config) -> Result<(Status,Vec<ContributorState>)> {
    let contributors = state.contributors_db.get_contributors().await?;
    let status = state.contributors_db.get_global_status().await?;
    Ok((status, contributors))
}


// TODO handle_remove? Need a way to cancel rayon task, in case verification is in progres...




