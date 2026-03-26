use chrono::{TimeDelta, Utc};
use common::contribution::Contributor;
use ed25519_dalek::VerifyingKey;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Deserializer};
use serde_json::json;

use crate::{error::ErrorWithCode, store::{contribution_files::{ContributionFileHandle, ContributionFilesStore}, contributors_db::{ContributorState, ContributorsDB, Status}}, verification_job::VerificationJob};
use crate::store::contributors_db::ContributorStatus;
use common::messages::Msg;

const PING_TIMEOUT : TimeDelta = TimeDelta::seconds(20);
const DOWNLOAD_TIMEOUT : TimeDelta = TimeDelta::seconds(60);
const COMPUTE_TIMEOUT : TimeDelta = TimeDelta::seconds(720);
const UPLOAD_TIMEOUT : TimeDelta = TimeDelta::seconds(120);

fn deserialize_verifying_key<'de, D: Deserializer<'de>>(deserializer: D) -> Result<VerifyingKey, D::Error> {
    let hex_str = String::deserialize(deserializer)?;
    let bytes: Vec<u8> = (0..hex_str.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex_str[i..i + 2], 16))
        .collect::<Result<Vec<u8>, _>>()
        .map_err(serde::de::Error::custom)?;
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| serde::de::Error::custom("verifying key must be 32 bytes"))?;
    VerifyingKey::from_bytes(&bytes).map_err(serde::de::Error::custom)
}

#[derive(Deserialize)]
pub struct Config {
    pub db_path: String,
    pub bucket_id: String,
    pub gcp_project_id: String,
    #[serde(deserialize_with = "deserialize_verifying_key")]
    pub admin_verifying_key: VerifyingKey,
    #[serde(default = "default_ping_timeout")]
    pub ping_timeout_secs: i64,
    #[serde(default = "default_download_timeout")]
    pub download_timeout_secs: i64,
    #[serde(default = "default_contribute_timeout")]
    pub contribute_timeout_secs: i64,
    #[serde(default = "default_upload_timeout")]
    pub upload_timeout_secs: i64,
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_ping_timeout() -> i64 { 20 }
fn default_download_timeout() -> i64 { 60 }
fn default_contribute_timeout() -> i64 { 720 }
fn default_upload_timeout() -> i64 { 120 }
fn default_port() -> u16 { 3000 }

impl Config {
    pub fn ping_timeout(&self) -> TimeDelta { TimeDelta::seconds(self.ping_timeout_secs) }
    pub fn download_timeout(&self) -> TimeDelta { TimeDelta::seconds(self.download_timeout_secs) }
    pub fn contribute_timeout(&self) -> TimeDelta { TimeDelta::seconds(self.contribute_timeout_secs) }
    pub fn upload_timeout(&self) -> TimeDelta { TimeDelta::seconds(self.upload_timeout_secs) }
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


pub async fn handle_join(c: &Contributor, state: &mut State, _config: &Config) -> Result<()> {
    match state.contributors_db.get_contributor_status(&c).await? {
        ContributorStatus::DidntJoinQueue 
        | ContributorStatus::Kicked {..} => {
            state.contributors_db.enqueue(&c).await
        },
        ContributorStatus::Queued {..} => {
            bail!("Already in queue")
        }
        ContributorStatus::Finished { .. } => {
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

pub async fn handle_download_all(state: &mut State, _config: &Config) -> Result<Vec<String>> {
    let contributors = state.contributors_db.get_finished_contributors().await?;
    let mut urls = Vec::with_capacity(contributors.len());
    for c in &contributors {
        let handle = state.contribution_files_store.get_or_create(c).await?;
        let url = handle.should_be_finished()?.as_client_url(&state.contribution_files_store).await?;
        urls.push(url);
    }
    Ok(urls)
}

// TODO handle_remove? Need a way to cancel rayon task, in case verification is in progres...

pub async fn handle(msg: Msg, state: &mut State, config: &Config) -> Result<serde_json::Value, ErrorWithCode> {
    Ok(match msg {
        Msg::Join { contributor } => {
            handle_join(&contributor, state, config).await?;
            json!(null)
        }
        Msg::GetStatus { contributor } => {
            let status = handle_get_status(&contributor, state, config).await?;
            match status {
                StatusResponse::DidntJoin => json!({"status": "didnt_join"}),
                StatusResponse::Kicked(err) => json!({"status": "kicked", "error": format!("{:#}", err)}),
                StatusResponse::WaitingInQueue(pos) => json!({"status": "waiting_in_queue", "position": pos}),
                StatusResponse::ReadyToDownloadPrevious(handle) => json!({
                    "status": "ready_to_download_previous",
                    "url": handle.as_client_url(&state.contribution_files_store).await?,
                }),
                StatusResponse::WaitingForContributionWithPrevious(handle) => json!({
                    "status": "waiting_for_contribution",
                    "url": handle.as_client_url(&state.contribution_files_store).await?,
                }),
                StatusResponse::ReadyForUpload(handle) => json!({
                    "status": "ready_for_upload",
                    "url": handle.as_client_url(&state.contribution_files_store).await?,
                }),
                StatusResponse::Verifying => json!({"status": "verifying"}),
                StatusResponse::Finished => json!({"status": "finished"}),
            }
        }
        Msg::UpdateDownloadProgress { finished, contributor } => {
            handle_update_download_progress(finished, contributor, state, config).await?;
            json!(null)
        }
        Msg::UpdateComputeProgress { finished, contributor } => {
            handle_update_compute_progress(finished, contributor, state, config).await?;
            json!(null)
        }
        Msg::UpdateUploadProgress { finished, contributor } => {
            handle_update_upload_progress(finished, contributor, state, config).await?;
            json!(null)
        }
        Msg::Register { contributor } => {
            handle_register(&contributor, state, config).await?;
            json!(null)
        }
        Msg::Report => {
            let (status, contributors) = handle_report(state, config).await?;
            json!({
                "status": status.variant_str(),
                "contributors": contributors.iter().map(|c| json!({
                    "name": c.contributor.name,
                    "email": c.contributor.email,
                    "status": match &c.status {
                        ContributorStatus::DidntJoinQueue => json!("didnt_join_queue"),
                        ContributorStatus::Queued { pos, .. } => json!({"queued": {"position": pos}}),
                        ContributorStatus::Kicked { err, .. } => json!({"kicked": {"error": format!("{:#}", err)}}),
                        ContributorStatus::Finished { when } => json!({"finished": {"when": when.to_rfc3339()}}),
                    },
                })).collect::<Vec<_>>(),
            })
        }
        Msg::DownloadAll => {
            let urls = handle_download_all(state, config).await?;
            json!(urls)
        }
    })
}
