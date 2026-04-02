use anyhow::{bail, Result};
use common::contribution::Contributor;
use common::messages::Msg;
use ed25519_dalek::SigningKey;
use rand::thread_rng;
use server::handlers::ReportResponse;
use server::config::Config;
use tokio::task::JoinSet;
use tracing::{info, warn, error};

use client::{contribute::{
    self, QueueOutcome,
}, smoke_test_latest::smoke_test_latest};

// ---------------------------------------------------------------------------
// Crash injection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CrashPhase {
    Download,
    Compute,
    Upload,
    Verify,
}

/// A modified version of `client::contribute::contribute` that can simulate a
/// crash at a specified phase by bailing out before the phase completes.
async fn test_contribute(
    sk: &SigningKey,
    me: &Contributor,
    crash_at: Option<CrashPhase>,
) -> Result<()> {
    match contribute::join_and_wait_in_queue(sk, me).await? {
        QueueOutcome::AlreadyFinished => return Ok(()),
        QueueOutcome::Verifying => {
            if matches!(crash_at, Some(CrashPhase::Verify)) {
                info!("[{}] Simulating crash during VERIFY", me.name);
                bail!("Simulated crash during verify");
            }
            contribute::wait_for_server_verification(sk, me).await?;
            return Ok(());
        }
        QueueOutcome::ReadyToDownload(maybe_url) => {
            // -- Download phase --
            if matches!(crash_at, Some(CrashPhase::Download)) {
                info!("[{}] Simulating crash during DOWNLOAD", me.name);
                bail!("Simulated crash during download");
            }

            let maybe_previous = match maybe_url {
                Some(url) => Some(contribute::download_previous(&url, sk, me).await?),
                None => None,
            };

            Msg::UpdateDownloadProgress { finished: true, contributor: me.clone() }
                .sign(sk)
                .send()
                .await?;

            // -- Compute phase --
            if matches!(crash_at, Some(CrashPhase::Compute)) {
                info!("[{}] Simulating crash during COMPUTE", me.name);
                bail!("Simulated crash during compute");
            }

            let my_contribution = contribute::compute_my_contribution(maybe_previous, sk, me).await?;

            Msg::UpdateComputeProgress { finished: true, contributor: me.clone() }
                .sign(sk)
                .send()
                .await?;

            // -- Upload phase --
            if matches!(crash_at, Some(CrashPhase::Upload)) {
                info!("[{}] Simulating crash during UPLOAD", me.name);
                bail!("Simulated crash during upload");
            }

            contribute::upload_my_contribution(&my_contribution, sk, me).await?;

            Msg::UpdateUploadProgress { finished: true, contributor: me.clone(), hash: format!("") }
                .sign(sk)
                .send()
                .await?;

            // -- Verify phase --
            if matches!(crash_at, Some(CrashPhase::Verify)) {
                info!("[{}] Simulating crash during VERIFY", me.name);
                bail!("Simulated crash during verify");
            }

            contribute::wait_for_server_verification(sk, me).await?;
        }
    }

    Ok(())
}

/// Runs a contributor to completion, retrying after a simulated crash.
async fn run_contributor(
    sk: SigningKey,
    contributor: Contributor,
    crash_at: Option<CrashPhase>,
) -> Result<()> {
    let mut crash = crash_at;
    loop {
        match test_contribute(&sk, &contributor, crash.take()).await {
            Ok(()) => {
                info!("[{}] Finished successfully!", contributor.name);
                return Ok(());
            }
            Err(e) => {
                //warn!("[{}] Error: {}. Waiting for timeout...", contributor.name, e);
                //tokio::time::sleep(Duration::from_secs(25)).await;
                //warn!("Retrying");
                warn!("[{}] Error: {}. Retrying...", contributor.name, e);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("Fatal panic: {info}");
        std::process::abort();
    }));

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let mut rng = thread_rng();

    // ---- Generate keypairs ----
    let admin_sk = SigningKey::generate(&mut rng);
    let admin_vk = admin_sk.verifying_key();

    let contributors: Vec<(SigningKey, Contributor)> = (0..10)
        .map(|i| Contributor::new(&format!("Contributor-{i}"), &format!("c{i}@test.com"), &mut rng))
        .collect();

    // ---- Server config ----
    let config = Box::new(Config {
        db_path: "sqlite::memory:".to_string(),
        bucket_id: "e2e-test-ceremony-bucket".to_string(),
        gcp_project_id: "benchmark-zkid-circuit".to_string(),
        admin_verifying_key: admin_vk,
        ping_timeout_secs: 20,
        download_timeout_secs: 30,
        contribute_timeout_secs: 300,
        upload_timeout_secs: 30,
        port: 0,
    });

    let config: &'static Config = Box::leak(config);

    // ---- Start server ----
    let (port, _server_handle) = server::serve::start_server(config).await?;
    let server_addr = format!("http://127.0.0.1:{port}");
    info!("Setting CEREMONY_SERVER_ADDRESS={server_addr}");
    unsafe { std::env::set_var("CEREMONY_SERVER_ADDRESS", &server_addr) };

    // ---- Register all contributors ----
    info!("Registering {} contributors...", contributors.len());
    for (_, contributor) in &contributors {
        Msg::Register { contributor: contributor.clone() }
            .sign(&admin_sk)
            .send()
            .await?;
    }
    info!("All contributors registered.");

    // ---- Define crash schedule ----
    let crash_schedule: Vec<Option<CrashPhase>> = vec![
        None,                       // 0: normal
        None,                       // 1: normal
        None,                       // 2: normal
        None,                       // 3: normal
        None,                       // 4: normal
        None,                       // 5: normal
        Some(CrashPhase::Download), // 6: crash during download
        Some(CrashPhase::Compute),  // 7: crash during compute
        Some(CrashPhase::Upload),   // 8: crash during upload
        Some(CrashPhase::Verify),   // 9: crash during verify
    ];

    // ---- Run all contributors concurrently ----
    info!("Starting {} contributors...", contributors.len());
    let mut join_set = JoinSet::new();

    for ((sk, contributor), crash) in contributors.iter().cloned().zip(crash_schedule) {
        let name = contributor.name.clone();
        let crash_label = crash.map_or("normal".to_string(), |c| format!("{c:?}"));
        info!("[{name}] Spawning (mode: {crash_label})");
        join_set.spawn(async move {
            run_contributor(sk, contributor, crash).await
        });
    }

    // Wait for all to finish
    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                error!("A contributor task failed: {e:#}");
                bail!("Test failed: {e:#}");
            }
            Err(e) => {
                error!("A contributor task panicked: {e}");
                bail!("Test failed: task panicked");
            }
        }
    }

    // ---- Verify via admin Report ----
    info!("All contributor tasks completed. Fetching report...");
    let report: ReportResponse = Msg::Report
        .sign(&admin_sk)
        .send_and_receive()
        .await?;

    let mut all_finished = true;
    for (_,row) in &report.contributors {
        let status_str = format!("{:?}", row.status);
        info!(
            "  {} <{}> — {}",
            row.name, row.email, status_str
        );
        if status_str != "Finished" {
            all_finished = false;
        }
    }

    if all_finished {
        info!("SUCCESS: All {} contributors finished!", report.contributors.len());

        smoke_test_latest(&admin_sk).await?;
    } else {
        bail!("FAILURE: Not all contributors finished!");
    }

    Ok(())
}
