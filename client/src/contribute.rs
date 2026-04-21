
use std::{process::{self, exit}, sync::Arc, time::{Duration, Instant}};

use anyhow::{Context, bail};
use common::{constants::{COMPUTE_TEST_CUTOFF, CeremonyContribution, DOWNLOAD_TEST_CUTOFF, PARAMS, TEST_PARAMS, UPLOAD_TEST_CUTOFF}, contribution::{Contribution, Contributor}, messages::{AuthenticatedMsg, Msg}};
use ed25519_dalek::SigningKey;
use rand::thread_rng;
use reqwest::StatusCode;
use server::handlers::StatusResponse;
use tokio::{sync::oneshot, task::JoinHandle};
use bytes::{Bytes, BytesMut};

const PING_INTERVAL : tokio::time::Duration = tokio::time::Duration::from_secs(5);

struct PingLoop {
    handle: JoinHandle<()>,
}

impl PingLoop {
    fn start(msg: AuthenticatedMsg<Msg>) -> Self {
        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(PING_INTERVAL);
            loop {
                ticker.tick().await;
                match msg.send().await {
                    Ok(()) => {eprint!(".")},
                    Err(e) => {
                        if let Some(StatusCode::GONE) = e.code {
                            eprintln!("{:?}", e.inner);
                        } else {
                            eprintln!("Eror pinging server: {:?}", e);
                        }
                        exit(1)
                    }
                }
            }
        });

        Self { handle } 
    }

    fn stop(&self) {
        self.handle.abort();
    }
}



pub enum QueueOutcome {
    ReadyToDownload(Option<String>),
    AlreadyFinished,
    Verifying,
}

pub fn print_step_outline() {
    eprintln!("");
    eprintln!("");
    eprintln!("You are about to contribute to the Aptos trusted setup ceremony. The contribution");
    eprintln!("will work as follows:");
    eprintln!("");
    eprintln!("");
    eprintln!("1. The client will test your download, compute, and upload speed, as contributing");
    eprintln!("   is computationally intensive and involves downloading and uploading over a GB.");
    eprintln!("");
    eprintln!("2. If the speed test passes, you will be added to the queue. Each contributor");
    eprintln!("   participates one at a time.");
    eprintln!("");
    eprintln!("3. Once it is your turn, the client will start your contribution. This consists of");
    eprintln!("   downloading the previous contribution, computing a new one, and uploading it to");
    eprintln!("   the server. This whole process could take up to 20 minutes depending on your");
    eprintln!("   computer's CPU power and internet connection speed.");
    eprintln!("");
    eprintln!("4. Once you've finished uploading, the server will verify if your contribution is");
    eprintln!("   valid. Assuming it passes verification, the contribution will be marked as");
    eprintln!("   finished, and you will be shown the path to your contribution file along with a");
    eprintln!("   contribution hash. Once the final ceremony result is published, you may use both");
    eprintln!("   of these items to verify the final ceremony contains your contribution. This");
    eprintln!("   verification takes around 4 minutes.");
    eprintln!("");
    eprintln!("Note: If your client becomes unresponsive, e.g., if your connection stalls, or if you");
    eprintln!("close your laptop during your turn, the server might kick you. In this case, you may");
    eprintln!("run the same command to rejoin the queue. To avoid this, please ensure that your computer");
    eprintln!("is on and connected to the internet during the whole duration of the contribution.");
    eprintln!("");
    eprintln!("");
}

pub async fn join_and_wait_in_queue(
    my_sk: &SigningKey,
    me: &Contributor,
    test_download_secs: u64,
    test_compute_secs: u64,
    test_upload_secs: u64
) -> anyhow::Result<QueueOutcome> {
    let mut interval = tokio::time::interval(PING_INTERVAL);
    let mut response = Msg::GetStatus { contributor: me.clone() }.sign(my_sk).send_and_receive::<StatusResponse>().await?;

    // loop that handles being in queue
    loop {
        match response {
            StatusResponse::DidntJoin => {
                Msg::Join { contributor: me.clone(), test_download_secs, test_compute_secs, test_upload_secs }.sign(my_sk).send().await?;
                eprintln!("Step 2: Joining queue.");
            },
            StatusResponse::Kicked(e) => {
                Msg::Join { contributor: me.clone(), test_download_secs, test_compute_secs, test_upload_secs }.sign(my_sk).send().await?;
                eprintln!("{}: Was kicked. Reason was {}. Rejoining queue.", me.name, e);
            },
            StatusResponse::WaitingInQueue(pos) => {
                eprintln!("You are at position {} in the queue.", pos);
            }
            StatusResponse::ReadyToDownloadPrevious(_) => break,
            StatusResponse::Finished => {
                eprintln!("You have already contributed to this ceremony.");
                return Ok(QueueOutcome::AlreadyFinished);
            },
            StatusResponse::Verifying => {
                eprintln!("Server already has your contribution and is verifying.");
                return Ok(QueueOutcome::Verifying);
            },
            _ => {
                eprintln!("Server thinks we are in the middle of contributing/uploading. Waiting ~25 secs for timeout...");
                tokio::time::sleep(Duration::from_secs(25)).await;
            },
        }
        interval.tick().await;
        response = Msg::GetStatus { contributor: me.clone() }.sign(my_sk).send_and_receive::<StatusResponse>().await?;
    }

    let StatusResponse::ReadyToDownloadPrevious(maybe_url) = response else {
        unreachable!()
    };

    Ok(QueueOutcome::ReadyToDownload(maybe_url))
}

pub async fn download_previous(url: &str, my_sk: &SigningKey, me: &Contributor) -> anyhow::Result<Bytes> {
    eprintln!("Step 3: It is your turn.");
    eprintln!("Downloading previous contribution...");
    // ping loop while downloading
    let ping_loop = PingLoop::start(
        Msg::UpdateDownloadProgress { finished: false, contributor: me.clone() }.sign(my_sk)
    );

    let bytes = reqwest::get(url)
        .await
        .context("Error while downloading previous contribution.")?
        .bytes().await
        .context("Error while downloading previous contribution.")?;

    ping_loop.stop();

    Ok(bytes)
}

pub async fn compute_my_contribution(maybe_previous_bytes: Option<Bytes>, my_sk: &SigningKey, me: &Contributor) -> anyhow::Result<Bytes> {
    eprintln!("Finished downloading, computing new contribution...");
    let ping_loop = PingLoop::start(
        Msg::UpdateComputeProgress { finished: false, contributor: me.clone() }.sign(my_sk)
    );

    let (tx, rx) = oneshot::channel::<Bytes>();
    let me_cloned = me.clone();

    rayon::spawn(move || {
        let mut rng = thread_rng();
        // deserialize here b/c its computationally expensive, and is done in parallel w/
        // rayon
        eprintln!("Deserializing previous contribution...");
        let start = Instant::now();
        let maybe_previous : Option<CeremonyContribution> = match maybe_previous_bytes { 
            Some(previous) => 
            Some(bcs::from_bytes(&previous)
                .context("Error while deserializing previous contribution.")
                .unwrap()),
            None => None 
        };
        eprintln!("Finished deserializing previous in {:?}", start.elapsed());

        eprintln!("Starting computation. Will take a while...");
        let start = Instant::now();
        let my_contribution = Contribution::generate(&mut rng, maybe_previous.as_ref(), &me_cloned, &PARAMS)
            .expect("There was a problem computing your contribution");
        eprintln!("Finished computing contribution in {:?}", start.elapsed());

        // serialize here b/c it's computationally expensive, and is done in parallel w/ rayon
        eprintln!("Serializing your contribution...");
        let start = Instant::now();
        let my_contribution_bytes = Bytes::from(bcs::to_bytes(&my_contribution)
            .expect("There was a problem serializing your contribution."));
        eprintln!("Finished serializing contribution in {:?}", start.elapsed());

        tx.send(my_contribution_bytes).expect("Should never fail to send")
    });

    let my_contribution = rx.await?;

    ping_loop.stop();

    Ok(my_contribution)
}


pub async fn upload_my_contribution(
    my_contribution: Arc<Bytes>, 
    my_sk: &SigningKey, 
    me: &Contributor) -> anyhow::Result<String> {
    eprintln!("Finished computing contribution, uploading...");

    let response = Msg::GetStatus { contributor: me.clone() }.sign(my_sk).send_and_receive::<StatusResponse>().await?;
    let StatusResponse::ReadyForUpload(part_urls) = response else {
        bail!("Finished compute, but server didn't give us the upload plan");
    };

    let ping_loop = PingLoop::start(
        Msg::UpdateUploadProgress { finished: false, hash: format!(""), contributor: me.clone() }.sign(my_sk)
    );

    tokio::fs::write("mine.contrib", my_contribution.as_ref()).await?;

    common::upload::upload_parallel(&part_urls, &my_contribution).await?;

    let hash = tokio::task::spawn_blocking(move ||
        blake3::hash(&my_contribution)
    ).await?.to_string();

    ping_loop.stop();

    Ok(hash)
}

pub async fn wait_for_server_verification(
    my_sk: &SigningKey,
    me: &Contributor,
) -> anyhow::Result<()> {
    eprintln!("Step 4: Finished uploading, waiting for server verification. This will take around 4 minutes...");
    let mut interval = tokio::time::interval(PING_INTERVAL);
    let mut response = Msg::GetStatus { contributor: me.clone() }.sign(my_sk).send_and_receive::<StatusResponse>().await?;

    // loop while server is verifying
    loop {
        match response {
            StatusResponse::Kicked(e) => {
                bail!("Kicked: {}", e);
            },
            StatusResponse::Finished => {
                eprintln!("Finished contributing!");
                return Ok(())
            },
            StatusResponse::Verifying => {
                eprintln!("Server is verifying...");
                interval.tick().await;
            },
            _ => {
                bail!("Unexpected status response: {:?}", response);
            }
        }
        interval.tick().await;
        response = Msg::GetStatus { contributor: me.clone() }.sign(my_sk).send_and_receive::<StatusResponse>().await?;
    }
}

pub async fn test_my_speed(my_sk: &SigningKey, me: &Contributor) -> anyhow::Result<(u64, u64, u64)> {

    let download_url : String = Msg::GetTestContributionDownloadLink { contributor: me.clone() }
        .sign(my_sk)
        .send_and_receive()
    .await?;

    eprintln!("Step 1: Testing your download speed...");

    let start = Instant::now();
    let mut bytes : BytesMut = BytesMut::from(reqwest::get(&download_url)
        .await
        .context("Error while downloading test contribution.")?
        .bytes().await
        .context("Error while downloading test contribution.")?);
    let download_duration = start.elapsed();
    eprintln!("Download took {:?}", download_duration);

    // NOTE we are being somewhat inaccurate here, b/c we aren't testing deserialize speed...
    eprintln!("Testing your compute speed...");

    let start = Instant::now();
    let my_test_contrib : CeremonyContribution = Contribution::generate(&mut thread_rng(), None, me, &*TEST_PARAMS)?;
    let compute_duration = start.elapsed();
    let my_test_contrib_bytes = bcs::to_bytes(&my_test_contrib)?;
    eprintln!("Compute took {:?}", compute_duration);

    bytes[..my_test_contrib_bytes.len()].copy_from_slice(&my_test_contrib_bytes);

    let part_urls : Vec<String> = Msg::GetTestContributionUploadLink { contributor: me.clone() }
        .sign(my_sk)
        .send_and_receive()
    .await?;

    eprintln!("Testing your upload speed...");

    let start = Instant::now();
    common::upload::upload_parallel(&part_urls, &bytes).await?;
    let upload_duration = start.elapsed();
    eprintln!("Upload took {:?}", upload_duration);

    let mut err_string = format!("One or more speed tests failed (shown below). Please use a faster connection and/or machine and try again.\n\n");
    let mut too_slow = false;
    if download_duration > *DOWNLOAD_TEST_CUTOFF {
        too_slow = true;
        err_string += &format!("Your download test was too slow; it took {:?}, whereas the cutoff is {:?}.\n", download_duration, *DOWNLOAD_TEST_CUTOFF);
    }
    if compute_duration > *COMPUTE_TEST_CUTOFF {
        too_slow = true;
        err_string += &format!("Your compute test was too slow; it took {:?}, whereas the cutoff is {:?}.\n", compute_duration, *COMPUTE_TEST_CUTOFF);
    }
    if upload_duration > *UPLOAD_TEST_CUTOFF {
        too_slow = true;
        err_string += &format!("Your upload test was too slow; it took {:?}, whereas the cutoff is {:?}.\n", upload_duration, *UPLOAD_TEST_CUTOFF);
    }

    if !too_slow {
        eprintln!("Speed test passed.");
        Ok((download_duration.as_secs(), compute_duration.as_secs(), upload_duration.as_secs()))
    } else {
        bail!(err_string)
    }
}

pub async fn contribute(my_sk: SigningKey, me: &Contributor) -> anyhow::Result<()> {
    eprintln!("Hello {}.", me.name);
    print_step_outline();

    let (download, compute, upload) = test_my_speed(&my_sk, me).await?;

    let maybe_url = match join_and_wait_in_queue(&my_sk, me, download, compute, upload).await? {
        QueueOutcome::AlreadyFinished => return Ok(()),
        QueueOutcome::ReadyToDownload(maybe_url) => maybe_url,
        QueueOutcome::Verifying => {
            wait_for_server_verification(&my_sk, me).await?;
            process::exit(0);
        }
    };

    let maybe_previous : Option<Bytes> = match maybe_url {
        Some(url) => {
            Some(download_previous(&url, &my_sk, me).await?)
        }
        None => None,
    };


    // tell server we're done downloading
    Msg::UpdateDownloadProgress { finished: true, contributor: me.clone() }
        .sign(&my_sk)
        .send()
    .await?;


    let my_contribution = Arc::new(compute_my_contribution(maybe_previous, &my_sk, me).await?);

    // tell server we're done computing
    Msg::UpdateComputeProgress { finished: true, contributor: me.clone() }.sign(&my_sk).send().await?;


    let hash = upload_my_contribution(my_contribution, &my_sk, me).await?;


    // tell server we're done uploading
    // TODO hash
    Msg::UpdateUploadProgress { finished: true, contributor: me.clone(), hash: hash.clone() }.sign(&my_sk).send().await?;


    wait_for_server_verification(&my_sk, me).await?;

    eprintln!("Finished contributing!");
    eprintln!("Your contribution hash is {}, and your contribution file is './mine.contrib'.", hash);
    eprintln!("You may use this file and hash to verify that the final output contains your contribution.");

    Ok(())
}

