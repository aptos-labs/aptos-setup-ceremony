use std::{sync::Arc};

use chrono::Utc;
use common::{constants::{PARAMS, test_upload_contributor}, contribution::{Contributor}};
use anyhow::{Result, anyhow};
use hyper::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::Mutex;

use crate::{bail, config::Config, error::{ErrorWithCode, UseCodeOnError}, store::{contribution_files::ContributionFilesStore, contributors_db::{ContributorsDB, types::{ContributorRow, ContributorStatus, CurrentContributionStep}}}, verification_job::VerificationJob};
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
    ReadyForUpload(Vec<String>),
    Verifying,
    Finished,
}

pub async fn handle_join(
    c: &Contributor,
    state: Arc<State>,
    _config: &Config,
    test_download_secs: u64,
    test_compute_secs: u64,
    test_upload_secs: u64,
) -> Result<(), ErrorWithCode> {
    let db_locked = state.contributors_db.lock().await;
    let (_, row) = db_locked.get_with_pos(&c).await?;
    match row.status {
        ContributorStatus::DidntJoinQueue 
        | ContributorStatus::Kicked {..} => {
            Ok(row.enqueue(&db_locked.pool, test_download_secs, test_compute_secs, test_upload_secs).await?)
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
                Ok(match row.get_current_contribution_step() {
                    CurrentContributionStep::DownloadNotStarted => {
                        // Note: we can keep track of the first time the client receives this
                        // response to know when it starts downloading
                        row.mark_started_download(&db_locked.pool).await?;
                        StatusResponse::ReadyToDownloadPrevious(
                            match db_locked.get_most_recent_finished_contributor().await? {
                                Some(prev_c) => Some(state.contribution_files_store.get_download_url(&prev_c).await?),
                                None => None,
                            }
                        )
                    },
                    CurrentContributionStep::DownloadStarted {..} => 
                    StatusResponse::ReadyToDownloadPrevious(
                        match db_locked.get_most_recent_finished_contributor().await? {
                            Some(prev_c) => Some(state.contribution_files_store.get_download_url(&prev_c).await?),
                            None => None,
                        }
                    ),
                    CurrentContributionStep::ComputeStarted {..} => 
                    StatusResponse::WaitingForContributionWithPrevious(
                        match db_locked.get_most_recent_finished_contributor().await? {
                            Some(prev_c) => Some(state.contribution_files_store.get_download_url(&prev_c).await?),
                            None => None,
                        }
                    ),
                    CurrentContributionStep::UploadStarted {..} =>
                    StatusResponse::ReadyForUpload(
                        state.contribution_files_store.get_upload_plan(&c).await?
                    ),
                    CurrentContributionStep::Verifying { .. } => 
                    StatusResponse::Verifying,
                    CurrentContributionStep::Finished => 
                    bail!("DB error: Contributor status is not marked finished, but verification was marked as finished")
                })
            }
        }
    }
}

pub fn check_correct_state(
    pos: usize,
    row: &ContributorRow,
    expected_step: CurrentContributionStep,
) -> Result<(), ErrorWithCode> {
    if row.status == ContributorStatus::Kicked {
        Err(anyhow!("You were kicked. Please restart to rejoin queue. Reason was: {}", row.kicked_error.as_ref().expect("Should always have a kicked error")))
            .use_code_on_error(StatusCode::GONE)
    } else if pos > 0 {
        Err(anyhow!("Not the current active contributor"))
            .use_code_on_error(StatusCode::GONE)
    } else if std::mem::discriminant(&row.get_current_contribution_step()) !=
    std::mem::discriminant(&expected_step) {
        Err(anyhow!(
            "Can't make this request right now, the current state is {}, expected {}.",
            row.get_current_contribution_step().variant_name(),
            expected_step.variant_name()
        )).use_code_on_error(StatusCode::BAD_REQUEST)
    } else {
        Ok(())
    }
}

pub async fn handle_update_download_progress(finished: bool, c: Contributor, state: Arc<State>, _config: &Config) -> Result<(), ErrorWithCode> {
    let db_locked = state.contributors_db.lock().await;
    let (pos, row) = db_locked.get_with_pos(&c).await?;
    check_correct_state(
        pos, 
        &row, 
        CurrentContributionStep::DownloadStarted { start: Utc::now() }
    )?;

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
    check_correct_state(
        pos, 
        &row, 
        CurrentContributionStep::ComputeStarted  { start: Utc::now() }
    )?;

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
    check_correct_state(
        pos, 
        &row, 
        CurrentContributionStep::UploadStarted { start: Utc::now() }
    )?;


    if finished {
        state.contribution_files_store.finalize_upload(&c).await?;
        row.mark_finished_upload(hash.clone(), &db_locked.pool).await?;
        let state_cloned = state.clone();
        let config_cloned = config.clone();
        tokio::spawn(async move {
            let verification_result = run_verify(c.clone(), hash, state_cloned.clone(), config_cloned).await;

            match verification_result {
                Ok(_) => {
                    state_cloned.contributors_db.lock().await.finish_current().await
                        .expect("This must succeed; otherwise, if it fails, I'm not sure how to report the error or keep the database in a valid state");
                    tracing::info!("Verification succeeded. Contributor {} marked as finished.", c.name);
                },
                Err(e) => {
                    let e_with_context = e.context("Verification of your contribution failed.");
                    state_cloned.contributors_db.lock().await.kick_current(
                        &format!("{:?}", e_with_context)
                    ).await
                        .expect("This must succeed; otherwise, if it fails, I'm not sure how to report the error or keep the database in a valid state");

                    tracing::info!("Verification failed: {:?}. Contributor {} kicked,", e_with_context, c.name);
                }
            }

        });
    } else {
        row.update_timestamp(&db_locked.pool).await?;
    }

    Ok(())
}


// Not a handler; invoked directly after upload finished
async fn run_verify(c: Contributor, hash: String, state: Arc<State>, _config: Config) -> Result<()> {
    let mut db_locked = state.contributors_db.lock().await;
    let maybe_previous = db_locked.get_most_recent_finished_contributor().await?;
    // drop lock before starting verification job, so we can respond to other requests during
    // verification
    drop(db_locked); 
    tracing::info!("Starting verification for {}", c.name);
    let current_verification_job = VerificationJob::start(&c, hash, &maybe_previous, &state.contribution_files_store, &PARAMS).await?; 
    let verification_result = current_verification_job.finished().await?;
    Ok(verification_result?)
}

pub async fn handle_tick(state: Arc<State>, config: &Config) -> Result<()> {
    tracing::info!("[Tick] Start tick");
    let db_locked = state.contributors_db.lock().await;
    let Some(current_contributor) = db_locked.get_current().await? else {
        tracing::info!("[Tick] No current contributor, doing nothing");
        return Ok(());
    };


    let current_time = Utc::now();

    if current_time - current_contributor.updated_timestamp > config.ping_timeout() {
        tracing::info!(
            "[Tick] Kicking contributor {}: ping time {} exceeded timeout of {}", 
            current_contributor.name, 
            current_time - current_contributor.updated_timestamp, 
            config.ping_timeout()
        );
        db_locked.kick_current(&format!(
            "Timed out: ping time {:?} exceeded timeout of {:?}", 
            current_time - current_contributor.updated_timestamp, 
            config.ping_timeout()
        )).await?;
        return Ok(());
    }
    match current_contributor.get_current_contribution_step() {
        CurrentContributionStep::DownloadNotStarted => {
            // On tick, if the active contributor hasn't started download, mark the
            // download started anyways 
            current_contributor.mark_started_download(&db_locked.pool).await?;
        },
        CurrentContributionStep::DownloadStarted { start } => {
            if current_time - start > config.download_timeout() {
                tracing::info!(
                    "[Tick] Kicking contributor {}: download time {} exceeded timeout of {}", 
                    current_contributor.name, 
                    current_time - start, 
                    config.download_timeout()
                );
                db_locked.kick_current(&format!("Timed out: download time {:?} exceeded timeout of {:?}", current_time - start, config.download_timeout())).await?;
            }
        }
        CurrentContributionStep::ComputeStarted { start } => {
            if current_time - start > config.contribute_timeout() {
                tracing::info!(
                    "[Tick] Kicking contributor {}: compute time {} exceeded timeout of {}", 
                    current_contributor.name, 
                    current_time - start, 
                    config.contribute_timeout()
                );
                db_locked.kick_current(&format!("Timed out: compute time {:?} exceeded timeout of {:?}", current_time - start, config.contribute_timeout())).await?;
            }
        }
        CurrentContributionStep::UploadStarted { start } => {
            if current_time - start > config.upload_timeout() {
                tracing::info!(
                    "[Tick] Kicking contributor {}: upload time {} exceeded timeout of {}", 
                    current_contributor.name, 
                    current_time - start, 
                    config.upload_timeout()
                );
                db_locked.kick_current(&format!("Timed out: upload time {:?} exceeded timeout of {:?}", current_time - start, config.upload_timeout())).await?;
            }
        }
        // We don't timeout contributors during verification, since even if they go offline we can
        // verify and mark their contribution as complete
        CurrentContributionStep::Verifying => (),
        CurrentContributionStep::Finished => (),
    }

    Ok(())
}


pub async fn handle_register(c: &Contributor, state: Arc<State>, _config: &Config) -> Result<()> {
    state.contributors_db.lock().await.register(c).await?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
pub struct ReportResponse {
    pub status: Option<CurrentContributionStep>,
    pub contributors: Vec<(usize,ContributorRow)>,
}

pub async fn handle_report(state: Arc<State>, _config: &Config) -> Result<ReportResponse> {
    let db_locked = state.contributors_db.lock().await;
    let contributors = db_locked.get_contributors().await?;
    let status = db_locked.get_current().await?.map(|c| c.get_current_contribution_step());
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

pub async fn handle_get_test_contribution_upload_link(state: Arc<State>, _config: &Config) -> Result<Vec<String>> {
    let urls = state.contribution_files_store.get_upload_plan(&test_upload_contributor()).await?;
    Ok(urls)
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
        Msg::Join { contributor, test_download_secs, test_compute_secs, test_upload_secs } => {
            handle_join(&contributor, state, config, test_download_secs, test_compute_secs, test_upload_secs).await?;
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
