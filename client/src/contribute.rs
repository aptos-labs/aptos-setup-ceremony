
use std::{process, time::{Duration, Instant}};

use anyhow::{Context, bail};
use common::{constants::{PARAMS, TEST_PARAMS, UPLOAD_CHUNK_SIZE}, contribution::{Contribution, Contributor}, fptx::FPTXContributionInner, messages::{AuthenticatedMsg, Msg}};
use ed25519_dalek::SigningKey;
use rand::thread_rng;
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
                    msg.send().await
                    .expect("Should never fail to ping");
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

pub async fn join_and_wait_in_queue(my_sk: &SigningKey, me: &Contributor) -> anyhow::Result<QueueOutcome> {
    let mut interval = tokio::time::interval(PING_INTERVAL);
    let mut response = Msg::GetStatus { contributor: me.clone() }.sign(my_sk).send_and_receive::<StatusResponse>().await?;

    // loop that handles being in queue
    loop {
        match response {
            StatusResponse::DidntJoin => {
                Msg::Join { contributor: me.clone() }.sign(my_sk).send().await?;
                eprintln!("Joining queue.");
            },
            StatusResponse::Kicked(e) => {
                Msg::Join { contributor: me.clone() }.sign(my_sk).send().await?;
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

pub async fn download_previous(url: &str, my_sk: &SigningKey, me: &Contributor) -> anyhow::Result<Contribution<FPTXContributionInner>> {
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

    bcs::from_bytes(&bytes)
        .context("Error while deserializing previous contribution.")
}

pub async fn compute_my_contribution(maybe_previous: Option<Contribution<FPTXContributionInner>>, my_sk: &SigningKey, me: &Contributor) -> anyhow::Result<Bytes> {
    eprintln!("Finished downloading, computing new contribution...");
    let ping_loop = PingLoop::start(
        Msg::UpdateComputeProgress { finished: false, contributor: me.clone() }.sign(my_sk)
    );

    let (tx, rx) = oneshot::channel::<Bytes>();
    let me_cloned = me.clone();

    rayon::spawn(move || {
        let mut rng = thread_rng();
        tx.send(
            // serialize here b/c it's computationally expensive, and is done in parallel w/ rayon
            Bytes::from(bcs::to_bytes(&Contribution::generate(&mut rng, maybe_previous.as_ref(), &me_cloned, &PARAMS))
            .expect("Should never fail to serialize"))
        ).expect("Should never fail to send")
    });

    let my_contribution = rx.await?;

    ping_loop.stop();

    Ok(my_contribution)
}

pub async fn upload_my_contribution(
    my_contribution: &Bytes, 
    my_sk: &SigningKey, 
    me: &Contributor) -> anyhow::Result<()> {
    eprintln!("Finished computing contribution, uploading...");

    let response = Msg::GetStatus { contributor: me.clone() }.sign(my_sk).send_and_receive::<StatusResponse>().await?;
    let StatusResponse::ReadyForUpload(session_url) = response else {
        bail!("Finished compute, but server didn't give us the session url for upload");
    };

    let ping_loop = PingLoop::start(
        Msg::UpdateUploadProgress { finished: false, contributor: me.clone() }.sign(my_sk)
    );


    common::upload::upload_chunked(
        &session_url,
        &my_contribution,
        UPLOAD_CHUNK_SIZE).await?;

    ping_loop.stop();

    Ok(())
}

pub async fn wait_for_server_verification(
    my_sk: &SigningKey,
    me: &Contributor,
) -> anyhow::Result<()> {
    eprintln!("Finished uploading, waiting for server verification...");
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

pub async fn test_my_speed(my_sk: &SigningKey, me: &Contributor) -> anyhow::Result<()> {

    let download_url : String = Msg::GetTestContributionDownloadLink { contributor: me.clone() }
        .sign(my_sk)
        .send_and_receive()
    .await?;

    eprintln!("Testing your download speed...");

    let start = Instant::now();
    let mut bytes : BytesMut = BytesMut::from(reqwest::get(&download_url)
        .await
        .context("Error while downloading test contribution.")?
        .bytes().await
        .context("Error while downloading test contribution.")?);
    let download_duration = start.elapsed();
    eprintln!("Download took {:?}", download_duration);

    eprintln!("Testing your compute speed...");

    let my_test_contrib : Contribution<FPTXContributionInner> = Contribution::generate(&mut thread_rng(), None, me, &TEST_PARAMS);
    let compute_duration = start.elapsed();
    let my_test_contrib_bytes = bcs::to_bytes(&my_test_contrib)?;
    eprintln!("Compute took {:?}", compute_duration);

    bytes[..my_test_contrib_bytes.len()].copy_from_slice(&my_test_contrib_bytes);

    let session_url : String = Msg::GetTestContributionUploadLink { contributor: me.clone() }
        .sign(my_sk)
        .send_and_receive()
    .await?;

    eprintln!("Testing your upload speed...");

    let start = Instant::now();
    common::upload::upload_chunked(
        &session_url, 
        &bytes, 
        UPLOAD_CHUNK_SIZE).await?;
    let upload_duration = start.elapsed();
    eprintln!("Upload took {:?}", upload_duration);


    Ok(())
}

pub async fn contribute(my_sk: SigningKey, me: &Contributor) -> anyhow::Result<()> {
    eprintln!("Hello {}.", me.name);

    test_my_speed(&my_sk, me).await?;

    let maybe_url = match join_and_wait_in_queue(&my_sk, me).await? {
        QueueOutcome::AlreadyFinished => return Ok(()),
        QueueOutcome::ReadyToDownload(maybe_url) => maybe_url,
        QueueOutcome::Verifying => {
            wait_for_server_verification(&my_sk, me).await?;
            process::exit(0);
        }
    };

    let maybe_previous : Option<Contribution<FPTXContributionInner>> = match maybe_url {
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


    let my_contribution = compute_my_contribution(maybe_previous, &my_sk, me).await?;

    // tell server we're done computing
    Msg::UpdateComputeProgress { finished: true, contributor: me.clone() }.sign(&my_sk).send().await?;


    upload_my_contribution(&my_contribution, &my_sk, me).await?;

    // tell server we're done uploading
    Msg::UpdateUploadProgress { finished: true, contributor: me.clone() }.sign(&my_sk).send().await?;


    wait_for_server_verification(&my_sk, me).await?;

    Ok(())
}

