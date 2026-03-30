use std::sync::Arc;

use chrono::Utc;
use common::{constants::{PARAMS, test_upload_contributor}, contribution::{Contributor}};
use anyhow::{Result, anyhow, Context};
use hyper::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::Mutex;

use crate::{config::Config, error::{ErrorWithCode, UseCodeOnError}, store::{contribution_files::ContributionFilesStore, contributors_db::{ContributorState, ContributorsDB, Status}}, verification_job::VerificationJob};
use crate::store::contributors_db::ContributorStatus;
use common::messages::Msg;



pub struct State {
    // Putting this behind a mutex instead of trying to use DB transactions,
    // because I don't want to deal with serialization failure/retry
    pub contributors_db: Arc<Mutex<ContributorsDB>>,
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

pub async fn handle_join(c: &Contributor, state: Arc<State>, _config: &Config) -> Result<(), ErrorWithCode> {
    let mut db_locked = state.contributors_db.lock().await;
    match db_locked.get_contributor_status(c).await? {
        ContributorStatus::DidntJoinQueue 
        | ContributorStatus::Kicked {..} => {
            Ok(db_locked.enqueue(c).await?)
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

pub async fn handle_get_status(c: &Contributor, state: Arc<State>, _config: &Config) -> Result<StatusResponse, ErrorWithCode> {
    let mut db_locked = state.contributors_db.lock().await;
    db_locked.update_timestamp(&c).await?;
    Ok(match db_locked.get_contributor_status(c).await
        .use_code_on_error(StatusCode::NOT_FOUND)? {
        ContributorStatus::DidntJoinQueue => StatusResponse::DidntJoin,
        ContributorStatus::Queued { joined: _, pos } => {
            if pos > 0 {
                StatusResponse::WaitingInQueue(pos)
            } else {
                match db_locked.get_global_status().await? {
                    crate::store::contributors_db::Status::WaitingForDownload {..} => 
                    StatusResponse::ReadyToDownloadPrevious(
                        match db_locked.get_most_recent_finished_contributor().await? {
                            Some(h) => Some(state.contribution_files_store.get_or_create(&h).await?
                                .should_be_finished()?
                                .as_client_url(&state.contribution_files_store).await?),
                            None => None,
                        }
                    ),
                    crate::store::contributors_db::Status::WaitingForCompute {..} => 
                    StatusResponse::WaitingForContributionWithPrevious(
                        match db_locked.get_most_recent_finished_contributor().await? {
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

pub async fn handle_update_download_progress(finished: bool, c: Contributor, state: Arc<State>, _config: &Config) -> Result<(), ErrorWithCode> {
    let mut db_locked = state.contributors_db.lock().await;
    let ContributorStatus::Queued { joined: _, pos: 0 } = db_locked.get_contributor_status(&c).await? else {
        return Err(anyhow!("Not the current active contributor"))
        .use_code_on_error(StatusCode::BAD_REQUEST)

    };
    let Status::WaitingForDownload{..} =  db_locked.get_global_status().await? else {
        return Err(anyhow!("Not currently downloading"))
        .use_code_on_error(StatusCode::BAD_REQUEST)
    };

    if finished {
        db_locked.set_global_status(&Status::WaitingForCompute { start: Utc::now() } ).await?;
    }
    db_locked.update_timestamp(&c).await?;

    Ok(())
}

pub async fn handle_update_compute_progress(finished: bool, c: Contributor, state: Arc<State>, _config: &Config) -> Result<(), ErrorWithCode> {
    let mut db_locked = state.contributors_db.lock().await;
    let ContributorStatus::Queued { joined: _, pos: 0 } = db_locked.get_contributor_status(&c).await? else {
        return Err(anyhow!("Not the current active contributor"))
        .use_code_on_error(StatusCode::BAD_REQUEST);
    };
    let Status::WaitingForCompute { .. } = db_locked.get_global_status().await? else {
        return Err(anyhow!("Not currently waiting for compute"))
        .use_code_on_error(StatusCode::BAD_REQUEST);
    };

    if finished {
        db_locked.set_global_status(&Status::WaitingForUpload { start: Utc::now() }).await?;
    }
    db_locked.update_timestamp(&c).await?;

    Ok(())
}

pub async fn handle_update_upload_progress(finished: bool, c: Contributor, state: Arc<State>, config: &Config) -> Result<(), ErrorWithCode> {
    let mut db_locked = state.contributors_db.lock().await;

    let ContributorStatus::Queued { joined: _, pos: 0 } = db_locked.get_contributor_status(&c).await? else {
        return Err(anyhow!("Not the current active contributor"))
            .use_code_on_error(StatusCode::BAD_REQUEST);
    };
    let Status::WaitingForUpload { .. } = db_locked.get_global_status().await? else {
        return Err(anyhow!("Not currently waiting for upload"))
            .use_code_on_error(StatusCode::BAD_REQUEST);
    };

    db_locked.update_timestamp(&c).await?;

    if finished {
        db_locked.set_global_status(&Status::Verifying { start: Utc::now() }).await?;
        let state_cloned = state.clone();
        let config_cloned = config.clone();
        tokio::spawn(handle_verify(c, state_cloned, config_cloned));
    }

    Ok(())
}


// Not a handler; invoked directly by 
async fn handle_verify(c: Contributor, state: Arc<State>, _config: Config) -> Result<()> {
    let mut db_locked = state.contributors_db.lock().await;
    let maybe_previous = db_locked.get_most_recent_finished_contributor().await?;
    // drop lock before starting verification job, so we can respond to other requests during
    // verification
    drop(db_locked); 
    let current_verification_job = VerificationJob::start(&c, &maybe_previous, &state.contribution_files_store, &PARAMS).await?; 
    let verification_result = current_verification_job.finished().await?;
    tracing::info!("Verification finished: {:?}", verification_result);
    match verification_result {
        Ok(_) => {
            state.contributors_db.lock().await.finish_current().await?;
        },
        Err(e) => {
            state.contributors_db.lock().await.kick_current(
                &anyhow::Error::new(e)
                .context("Verification of your contribution failed.")
            ).await?;
        }
    }
    Ok(())
}

pub async fn handle_tick(state: Arc<State>, config: &Config) -> Result<()> {
    tracing::info!("Tick");
    let mut db_locked = state.contributors_db.lock().await;
    let Some(current_contributor) = db_locked.get_current().await? else {
        tracing::info!("No current contributor, doing nothing");
        return Ok(());
    };

    let status = db_locked.get_global_status().await?;

    let current_time = Utc::now();

    if current_time - current_contributor.updated_timestamp > config.ping_timeout() {
        tracing::info!(
            "Kicking contributor {}: ping time {} exceeded timeout of {}", 
            current_contributor.contributor.name, 
            current_time - current_contributor.updated_timestamp, 
            config.ping_timeout()
        );
        db_locked.kick_current(&anyhow::anyhow!("Timed out")).await?;
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
                db_locked.kick_current(&anyhow::anyhow!("Timed out")).await?;
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
                db_locked.kick_current(&anyhow::anyhow!("Timed out")).await?;
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
                db_locked.kick_current(&anyhow::anyhow!("Timed out")).await?;
            }
        }
        // We don't timeout contributors during verification, since even if they go offline we can
        // verify and mark their contribution as complete
        Status::Verifying { .. } => (),
    }

    Ok(())
}


pub async fn handle_register(c: &Contributor, state: Arc<State>, _config: &Config) -> Result<()> {
    state.contributors_db.lock().await.register(c).await?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
pub struct ReportResponse {
    pub status: Status,
    pub contributors: Vec<ContributorState>,
}

pub async fn handle_report(state: Arc<State>, _config: &Config) -> Result<ReportResponse> {
    let db_locked = state.contributors_db.lock().await;
    let contributors = db_locked.get_contributors().await?;
    let status = db_locked.get_global_status().await?;
    Ok(ReportResponse { status, contributors })
}

pub async fn handle_download_all(state: Arc<State>, _config: &Config) -> Result<Vec<String>> {
    let db_locked = state.contributors_db.lock().await;
    let contributors = db_locked.get_finished_contributors().await?;
    drop(db_locked);
    let mut urls = Vec::with_capacity(contributors.len());
    for c in &contributors {
        let handle = state.contribution_files_store.get_or_create(c).await?;
        let url = handle.should_be_finished()?.as_client_url(&state.contribution_files_store).await?;
        urls.push(url);
    }
    Ok(urls)
}

pub async fn handle_get_test_contribution_download_link(state: Arc<State>, _config: &Config) -> Result<String> {
    let url = state.contribution_files_store.get_test_blob_download_url().await?;
    Ok(url)
}

pub async fn handle_get_test_contribution_upload_link(state: Arc<State>, _config: &Config) -> Result<String> {
    let handle = state.contribution_files_store.get_or_create(&test_upload_contributor()).await?;
    let url = handle.should_not_be_finished()?.as_client_url(&state.contribution_files_store).await?;
    Ok(url)
}

pub async fn handle_download_latest(state: Arc<State>, _config: &Config) -> Result<String, ErrorWithCode> {
    match state.contributors_db.lock().await.get_most_recent_finished_contributor().await? {
        Some(h) => Ok(state.contribution_files_store.get_or_create(&h).await?
            .should_be_finished()?
            .as_client_url(&state.contribution_files_store).await?),
        None => {
            Err(anyhow::anyhow!("No contributions yet"))
                .use_code_on_error(StatusCode::NOT_FOUND)
        }
    }
}

pub async fn handle(msg: Msg, state: Arc<State>, config: &Config) -> Result<serde_json::Value, ErrorWithCode> {

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
        },
        Msg::DownloadLatest => {
            let url = handle_download_latest(state, config).await?;
            json!(url)
        },
        Msg::GetTestContributionDownloadLink { .. } => {
            json!(handle_get_test_contribution_download_link(state, config).await?)
        },
        Msg::GetTestContributionUploadLink { .. } => {
            json!(handle_get_test_contribution_upload_link(state, config).await?)
        },
    })
}
