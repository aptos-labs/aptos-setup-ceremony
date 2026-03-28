use chrono::Utc;
use common::{constants::PARAMS, contribution::Contributor};
use anyhow::{Result, anyhow, Context};
use hyper::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{config::Config, error::{ErrorWithCode, UseCodeOnError}, store::{contribution_files::ContributionFilesStore, contributors_db::{ContributorState, ContributorsDB, Status}}, verification_job::VerificationJob};
use crate::store::contributors_db::ContributorStatus;
use common::messages::Msg;



pub struct State {
    pub contributors_db: ContributorsDB,
    pub contribution_files_store: ContributionFilesStore,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum StatusResponse {
    DidntJoin,
    Kicked(String),
    WaitingInQueue(usize),
    ReadyToDownloadPrevious(Option<String>),
    WaitingForContributionWithPrevious(Option<String>),
    ReadyForUpload(String),
    Verifying,
    Finished,
}

pub async fn handle_join(c: &Contributor, state: &mut State, _config: &Config) -> Result<(), ErrorWithCode> {
    match state.contributors_db.get_contributor_status(c).await? {
        ContributorStatus::DidntJoinQueue 
        | ContributorStatus::Kicked {..} => {
            Ok(state.contributors_db.enqueue(c).await?)
        },
        ContributorStatus::Queued {..} => {
            Err(anyhow!("Already in queue"))
            .use_code_on_error(StatusCode::BAD_REQUEST)
        }
        ContributorStatus::Finished { .. } => {
            Err(anyhow!("Already finished contributing"))
            .use_code_on_error(StatusCode::BAD_REQUEST)
        }
    }
}

pub async fn handle_get_status(c: &Contributor, state: &mut State, _config: &Config) -> Result<StatusResponse, ErrorWithCode> {
    state.contributors_db.update_timestamp(&c).await?;
    Ok(match state.contributors_db.get_contributor_status(c).await
        .use_code_on_error(StatusCode::NOT_FOUND)? {
        ContributorStatus::DidntJoinQueue => StatusResponse::DidntJoin,
        ContributorStatus::Queued { joined: _, pos } => {
            if pos > 0 {
                StatusResponse::WaitingInQueue(pos)
            } else {
                match state.contributors_db.get_global_status().await? {
                    crate::store::contributors_db::Status::WaitingForDownload {..} => 
                    StatusResponse::ReadyToDownloadPrevious(
                        match state.contributors_db.get_most_recent_finished_contributor().await? {
                            Some(h) => Some(state.contribution_files_store.get_or_create(&h).await?
                                .should_be_finished()?
                                .as_client_url(&state.contribution_files_store).await?),
                            None => None,
                        }
                    ),
                    crate::store::contributors_db::Status::WaitingForCompute {..} => 
                    StatusResponse::WaitingForContributionWithPrevious(
                        match state.contributors_db.get_most_recent_finished_contributor().await? {
                            Some(h) => Some(state.contribution_files_store.get_or_create(&h).await?
                                .should_be_finished()?
                                .as_client_url(&state.contribution_files_store).await?),
                            None => None,
                        }
                    ),
                    crate::store::contributors_db::Status::WaitingForUpload {..} => 
                    StatusResponse::ReadyForUpload(
                        state.contribution_files_store.create_or_overwrite(c).await?
                            .should_not_be_finished()
                            .context("While constructing URL for uploading current contribution")?
                            .as_client_url(&state.contribution_files_store).await?
                    ),
                    crate::store::contributors_db::Status::Verifying { .. } => 
                    StatusResponse::Verifying,
                }
            }
        },
        ContributorStatus::Kicked { err, .. } => StatusResponse::Kicked(format!("{}", err)),
        ContributorStatus::Finished { .. } => StatusResponse::Finished,
    })
}

pub async fn handle_update_download_progress(finished: bool, c: Contributor, state: &mut State, _config: &Config) -> Result<(), ErrorWithCode> {
    let ContributorStatus::Queued { joined: _, pos: 0 } = state.contributors_db.get_contributor_status(&c).await? else {
        return Err(anyhow!("Not the current active contributor"))
        .use_code_on_error(StatusCode::BAD_REQUEST)

    };
    let Status::WaitingForDownload{..} =  state.contributors_db.get_global_status().await? else {
        return Err(anyhow!("Not currently downloading"))
        .use_code_on_error(StatusCode::BAD_REQUEST)
    };

    if finished {
        state.contributors_db.set_global_status(Status::WaitingForCompute { start: Utc::now() } ).await?;
    }
    state.contributors_db.update_timestamp(&c).await?;

    Ok(())
}

pub async fn handle_update_compute_progress(finished: bool, c: Contributor, state: &mut State, _config: &Config) -> Result<(), ErrorWithCode> {
    let ContributorStatus::Queued { joined: _, pos: 0 } = state.contributors_db.get_contributor_status(&c).await? else {
        return Err(anyhow!("Not the current active contributor"))
        .use_code_on_error(StatusCode::BAD_REQUEST);
    };
    let Status::WaitingForCompute { .. } = state.contributors_db.get_global_status().await? else {
        return Err(anyhow!("Not currently waiting for compute"))
        .use_code_on_error(StatusCode::BAD_REQUEST);
    };

    if finished {
        state.contributors_db.set_global_status(Status::WaitingForUpload { start: Utc::now() }).await?;
    }
    state.contributors_db.update_timestamp(&c).await?;

    Ok(())
}

pub async fn handle_update_upload_progress(finished: bool, c: Contributor, state: &mut State, _config: &Config) -> Result<(), ErrorWithCode> {
    let ContributorStatus::Queued { joined: _, pos: 0 } = state.contributors_db.get_contributor_status(&c).await? else {
        return Err(anyhow!("Not the current active contributor"))
        .use_code_on_error(StatusCode::BAD_REQUEST);
    };
    let Status::WaitingForUpload { .. } = state.contributors_db.get_global_status().await? else {
        return Err(anyhow!("Not currently waiting for upload"))
        .use_code_on_error(StatusCode::BAD_REQUEST);
    };

    state.contributors_db.update_timestamp(&c).await?;
    if finished {
        let maybe_previous = state.contributors_db.get_most_recent_finished_contributor().await?;
        state.contributors_db.set_global_status(Status::Verifying { start: Utc::now() }).await?;
        let current_verification_job = VerificationJob::start(&c, &maybe_previous, &state.contribution_files_store, &PARAMS).await?; 
        match current_verification_job.finished().await {
            Ok(_) => {
                state.contributors_db.finish_current().await?;
            },
            Err(e) => {
                state.contributors_db.kick_current(&e).await?;
            }
        }
    }

    Ok(())
}


pub async fn handle_tick(state: &mut State, config: &Config) -> Result<()> {
    tracing::info!("Tick");
    let Some(current_contributor) = state.contributors_db.get_current().await? else {
        tracing::info!("No current contributor, doing nothing");
        return Ok(());
    };

    let status = state.contributors_db.get_global_status().await?;

    let current_time = Utc::now();

    if current_time - current_contributor.updated_timestamp > config.ping_timeout() {
        tracing::info!(
            "Kicking contributor {}: ping time {} exceeded timeout of {}", 
            current_contributor.contributor.name, 
            current_time - current_contributor.updated_timestamp, 
            config.ping_timeout()
        );
        state.contributors_db.kick_current(&anyhow::anyhow!("Timed out")).await?;
        return Ok(());
    }
    match status {
        Status::WaitingForDownload { start } => {
            if current_time - start > config.download_timeout() {
                tracing::info!(
                    "Kicking contributor {}: download time {} exceeded timeout of {}", 
                    current_contributor.contributor.name, 
                    current_time - start, 
                    config.download_timeout()
                );
                state.contributors_db.kick_current(&anyhow::anyhow!("Timed out")).await?;
            }
        }
        Status::WaitingForCompute { start } => {
            if current_time - start > config.contribute_timeout() {
                tracing::info!(
                    "Kicking contributor {}: compute time {} exceeded timeout of {}", 
                    current_contributor.contributor.name, 
                    current_time - start, 
                    config.contribute_timeout()
                );
                state.contributors_db.kick_current(&anyhow::anyhow!("Timed out")).await?;
            }
        }
        Status::WaitingForUpload { start } => {
            if current_time - start > config.upload_timeout() {
                tracing::info!(
                    "Kicking contributor {}: download time {} exceeded timeout of {}", 
                    current_contributor.contributor.name, 
                    current_time - start, 
                    config.upload_timeout()
                );
                state.contributors_db.kick_current(&anyhow::anyhow!("Timed out")).await?;
            }
        }
        Status::Verifying { .. } => (),
    }

    Ok(())
}


pub async fn handle_register(c: &Contributor, state: &mut State, _config: &Config) -> Result<()> {
    state.contributors_db.register(c).await?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
pub struct ReportResponse {
    pub status: Status,
    pub contributors: Vec<ContributorState>,
}

pub async fn handle_report(state: &mut State, _config: &Config) -> Result<ReportResponse> {
    let contributors = state.contributors_db.get_contributors().await?;
    let status = state.contributors_db.get_global_status().await?;
    Ok(ReportResponse { status, contributors })
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


pub async fn handle(msg: Msg, state: &mut State, config: &Config) -> Result<serde_json::Value, ErrorWithCode> {

    tracing::info!("Handling request {}", msg.description());

    Ok(match msg {
        Msg::Join { contributor } => {
            handle_join(&contributor, state, config).await?;
            json!("ok")
        }
        Msg::GetStatus { contributor } => {
            json!(
                handle_get_status(&contributor, state, config).await?
                )
        }
        Msg::UpdateDownloadProgress { finished, contributor } => {
            handle_update_download_progress(finished, contributor, state, config).await?;
            json!("ok")
        }
        Msg::UpdateComputeProgress { finished, contributor } => {
            handle_update_compute_progress(finished, contributor, state, config).await?;
            json!("ok")
        }
        Msg::UpdateUploadProgress { finished, contributor } => {
            handle_update_upload_progress(finished, contributor, state, config).await?;
            json!("ok")
        }
        Msg::Register { contributor } => {
            handle_register(&contributor, state, config).await?;
            json!("ok")
        }
        Msg::Report => {
            json!(
                handle_report(state, config).await?
                )
        }
        Msg::DownloadAll => {
            let urls = handle_download_all(state, config).await?;
            json!(urls)
        }
    })
}
