use std::{sync::Arc};

use chrono::Utc;
use common::{constants::{PARAMS, test_upload_contributor}, contribution::{Contributor}};
use anyhow::{Result, anyhow};
use hyper::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::Mutex;

use crate::{config::Config, error::{ErrorWithCode, UseCodeOnError}, store::{contribution_files::ContributionFilesStore, contributors_db::{ContributorsDB, types::{ContributorRow, ContributorStatus, GlobalStatus}}}, verification_job::VerificationJob};
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
    let db_locked = state.contributors_db.lock().await;
    let (_, row) = db_locked.get_with_pos(&c).await?;
    match row.status {
        ContributorStatus::DidntJoinQueue 
        | ContributorStatus::Kicked {..} => {
            Ok(row.enqueue(&db_locked.pool).await?)
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
    let (pos, row) = db_locked.get_with_pos(c).await?;
    row.clone().update_timestamp(&db_locked.pool).await?;

    match row.status {
        ContributorStatus::DidntJoinQueue => {
            Ok(StatusResponse::DidntJoin)

        }, 
        ContributorStatus::Kicked => {
            Ok(StatusResponse::Kicked(row.kicked_error.unwrap_or(format!(""))))
        },
        ContributorStatus::Finished => {
            Ok(StatusResponse::Finished)

        },  ContributorStatus::Queued => {
            if pos > 0 {
                Ok(StatusResponse::WaitingInQueue(pos))
            } else {
                Ok(match db_locked.get_global_status().await? {
                    GlobalStatus::WaitingForDownload {..} => 
                    // Note: we can keep track of the first time the client receives this
                    // response to know when it starts downloading
                    StatusResponse::ReadyToDownloadPrevious(
                        match db_locked.get_most_recent_finished_contributor().await? {
                            Some(prev_c) => Some(state.contribution_files_store.get_download_url(&prev_c).await?),
                            None => None,
                        }
                    ),
                    GlobalStatus::WaitingForCompute {..} => 
                    StatusResponse::WaitingForContributionWithPrevious(
                        match db_locked.get_most_recent_finished_contributor().await? {
                            Some(prev_c) => Some(state.contribution_files_store.get_download_url(&prev_c).await?),
                            None => None,
                        }
                    ),
                    GlobalStatus::WaitingForUpload {..} => 
                    StatusResponse::ReadyForUpload(
                        state.contribution_files_store.get_upload_url(&c).await?
                    ),
                    GlobalStatus::Verifying { .. } => 
                    StatusResponse::Verifying,
                })
            }
        }
    }
}

pub async fn handle_update_download_progress(finished: bool, c: Contributor, state: Arc<State>, _config: &Config) -> Result<(), ErrorWithCode> {
    let db_locked = state.contributors_db.lock().await;
    let (pos, row) = db_locked.get_with_pos(&c).await?;
    if pos > 0 {
        return Err(anyhow!("Not the current active contributor"))
            .use_code_on_error(StatusCode::GONE);
    }
    let GlobalStatus::WaitingForDownload{..} =  db_locked.get_global_status().await? else {
        return Err(anyhow!("Not currently downloading"))
            .use_code_on_error(StatusCode::BAD_REQUEST)
    };

    if finished {
        row.mark_started_compute(&db_locked.pool).await?;
    } else {
        row.update_timestamp(&db_locked.pool).await?;
    }

    Ok(())
}

pub async fn handle_update_compute_progress(finished: bool, c: Contributor, state: Arc<State>, _config: &Config) -> Result<(), ErrorWithCode> {
    let db_locked = state.contributors_db.lock().await;
    let (pos, row) = db_locked.get_with_pos(&c).await?;
    if pos > 0 {
        return Err(anyhow!("Not the current active contributor"))
            .use_code_on_error(StatusCode::GONE);
    };
    let GlobalStatus::WaitingForCompute { .. } = db_locked.get_global_status().await? else {
        return Err(anyhow!("Not currently waiting for compute"))
            .use_code_on_error(StatusCode::BAD_REQUEST);
    };

    if finished {
        row.mark_started_upload(&db_locked.pool).await?;
    } else {
        row.update_timestamp(&db_locked.pool).await?;
    }

    Ok(())
}

pub async fn handle_update_upload_progress(finished: bool, hash: String, c: Contributor, state: Arc<State>, config: &Config) -> Result<(), ErrorWithCode> {
    let db_locked = state.contributors_db.lock().await;
    let (pos, row) = db_locked.get_with_pos(&c).await?;
    if pos > 0 {
        return Err(anyhow!("Not the current active contributor"))
            .use_code_on_error(StatusCode::GONE);
    };
    let GlobalStatus::WaitingForUpload { .. } = db_locked.get_global_status().await? else {
        return Err(anyhow!("Not currently waiting for upload"))
            .use_code_on_error(StatusCode::BAD_REQUEST);
    };


    if finished {
        row.mark_finished_upload(hash.clone(), &db_locked.pool).await?;
        let state_cloned = state.clone();
        let config_cloned = config.clone();
        tokio::spawn(handle_verify(c, hash, state_cloned, config_cloned));
    } else {
        row.update_timestamp(&db_locked.pool).await?;
    }

    Ok(())
}


// Not a handler; invoked directly after upload finished
async fn handle_verify(c: Contributor, hash: String, state: Arc<State>, _config: Config) -> Result<()> {
    let mut db_locked = state.contributors_db.lock().await;
    let maybe_previous = db_locked.get_most_recent_finished_contributor().await?;
    // drop lock before starting verification job, so we can respond to other requests during
    // verification
    drop(db_locked); 
    tracing::info!("Starting verification for {}", c.name);
    let current_verification_job = VerificationJob::start(&c, hash, &maybe_previous, &state.contribution_files_store, &PARAMS).await?; 
    let verification_result = current_verification_job.finished().await?;
    match verification_result {
        Ok(_) => {
            state.contributors_db.lock().await.finish_current().await?;
            tracing::info!("Verification succeeded. Contributor {} marked as finished.", c.name);
        },
        Err(e) => {
            state.contributors_db.lock().await.kick_current(
                &format!("{:?}", anyhow::Error::new(e.clone())
                    .context("Verification of your contribution failed."))
            ).await?;
            tracing::info!("Verification failed: {:?}. Contributor {} kicked,", e, c.name);
        }
    }
    Ok(())
}

pub async fn handle_tick(state: Arc<State>, config: &Config) -> Result<()> {
    tracing::info!("Tick");
    let db_locked = state.contributors_db.lock().await;
    let Some(current_contributor) = db_locked.get_current().await? else {
        tracing::info!("No current contributor, doing nothing");
        return Ok(());
    };

    let status = db_locked.get_global_status().await?;

    let current_time = Utc::now();

    if current_time - current_contributor.updated_timestamp > config.ping_timeout() {
        tracing::info!(
            "Kicking contributor {}: ping time {} exceeded timeout of {}", 
            current_contributor.name, 
            current_time - current_contributor.updated_timestamp, 
            config.ping_timeout()
        );
        db_locked.kick_current(&format!("Timed out")).await?;
        return Ok(());
    }
    match status {
        GlobalStatus::WaitingForDownload { start } => {
            if current_time - start > config.download_timeout() {
                tracing::info!(
                    "Kicking contributor {}: download time {} exceeded timeout of {}", 
                    current_contributor.name, 
                    current_time - start, 
                    config.download_timeout()
                );
                db_locked.kick_current(&format!("Timed out")).await?;
            }
        }
        GlobalStatus::WaitingForCompute { start } => {
            if current_time - start > config.contribute_timeout() {
                tracing::info!(
                    "Kicking contributor {}: compute time {} exceeded timeout of {}", 
                    current_contributor.name, 
                    current_time - start, 
                    config.contribute_timeout()
                );
                db_locked.kick_current(&format!("Timed out")).await?;
            }
        }
        GlobalStatus::WaitingForUpload { start } => {
            if current_time - start > config.upload_timeout() {
                tracing::info!(
                    "Kicking contributor {}: download time {} exceeded timeout of {}", 
                    current_contributor.name, 
                    current_time - start, 
                    config.upload_timeout()
                );
                db_locked.kick_current(&format!("Timed out")).await?;
            }
        }
        // We don't timeout contributors during verification, since even if they go offline we can
        // verify and mark their contribution as complete
        GlobalStatus::Verifying => (),
    }

    Ok(())
}


pub async fn handle_register(c: &Contributor, state: Arc<State>, _config: &Config) -> Result<()> {
    state.contributors_db.lock().await.register(c).await?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
pub struct ReportResponse {
    pub status: GlobalStatus,
    pub contributors: Vec<(usize,ContributorRow)>,
}

pub async fn handle_report(state: Arc<State>, _config: &Config) -> Result<ReportResponse> {
    let db_locked = state.contributors_db.lock().await;
    let contributors = db_locked.get_contributors().await?;
    let status = db_locked.get_global_status().await?;
    Ok(ReportResponse { status, contributors })
}

pub async fn handle_download_all(state: Arc<State>, _config: &Config) -> Result<Vec<(ContributorRow, String)>> {
    let db_locked = state.contributors_db.lock().await;
    let contributors = db_locked.get_finished_contributors().await?;
    drop(db_locked);
    let mut urls = Vec::with_capacity(contributors.len());
    for c in &contributors {
        urls.push(state.contribution_files_store.get_download_url(&c.contributor()).await?);
    }
    Ok(contributors.into_iter().zip(urls).collect())
}

pub async fn handle_get_test_contribution_download_link(state: Arc<State>, _config: &Config) -> Result<String> {
    let url = state.contribution_files_store.get_test_blob_download_url().await?;
    Ok(url)
}

pub async fn handle_get_test_contribution_upload_link(state: Arc<State>, _config: &Config) -> Result<String> {
    let url = state.contribution_files_store.get_upload_url(&test_upload_contributor()).await?;
    Ok(url)
}

pub async fn handle_download_latest(state: Arc<State>, _config: &Config) -> Result<String, ErrorWithCode> {
    match state.contributors_db.lock().await.get_most_recent_finished_contributor().await? {
        Some(latest_c) => Ok(state.contribution_files_store.get_download_url(&latest_c).await?),
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
        Msg::UpdateUploadProgress { finished, contributor, hash } => {
            handle_update_upload_progress(finished, hash, contributor, state, config).await?;
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
