
use anyhow::bail;
use common::{contribution::{Contribution, Contributor}, fptx::{FPTXContributionInner, FPTXParams}, messages::Msg};
use ed25519_dalek::SigningKey;
use rand::thread_rng;
use server::handlers::StatusResponse;
use tokio::sync::oneshot;

use crate::upload;




pub async fn contribute(my_sk: SigningKey, me: &Contributor) -> anyhow::Result<()> {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
    let mut response = Msg::GetStatus { contributor: me.clone() }.sign(&my_sk).send_and_receive::<StatusResponse>().await?;

    // loop that handles being in queue
    loop {
        match response {
            StatusResponse::DidntJoin => {
                Msg::Join { contributor: me.clone() }.sign(&my_sk).send().await?;
                eprintln!("Joining queue.");
                interval.tick().await;
            },
            StatusResponse::Kicked(_) => {
                Msg::Join { contributor: me.clone() }.sign(&my_sk).send().await?;
                eprintln!("Was kicked. Rejoining queue.");
                interval.tick().await;
            },
            StatusResponse::WaitingInQueue(pos) => {
                eprintln!("You are at position {} in the queue.", pos);
                interval.tick().await;
            }
            StatusResponse::ReadyToDownloadPrevious(_) => break,
            _ => {
                bail!("Unexpected status response: {:?}", response);
            }
        }

        response = Msg::GetStatus { contributor: me.clone() }.sign(&my_sk).send_and_receive::<StatusResponse>().await?;
    }

    let StatusResponse::ReadyToDownloadPrevious(maybe_url) = response else {
        unreachable!()
    };

    let maybe_previous : Option<Contribution<FPTXContributionInner>> = match maybe_url {
        Some(url) => {

            // ping loop while downloading
            let me_cloned = me.clone();
            let my_sk_cloned = my_sk.clone();
            let handle = tokio::spawn(async move {
                let mut ticker = tokio::time::interval(tokio::time::Duration::from_secs(5));
                loop {
                    ticker.tick().await;
                    Msg::UpdateDownloadProgress { finished: false, contributor: me_cloned.clone() }.sign(&my_sk_cloned).send().await
                    .expect("Should never fail to ping");
                }
            });

            let bytes = reqwest::get(url).await?.bytes().await?;
            handle.abort();
            bcs::from_bytes(&bytes)?
        }
        None => None,
    };


    // tell server we're done downloading
    Msg::UpdateDownloadProgress { finished: true, contributor: me.clone() }.sign(&my_sk).send().await?;



    // ping loop while computing
    let me_cloned = me.clone();
    let my_sk_cloned = my_sk.clone();
    let handle = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(tokio::time::Duration::from_secs(5));
        loop {
            ticker.tick().await;
            Msg::UpdateComputeProgress { finished: false, contributor: me_cloned.clone() }.sign(&my_sk_cloned).send().await
                .expect("Should never fail to ping");
        }
    });

    let (tx, rx) = oneshot::channel::<Contribution<FPTXContributionInner>>();
    let me_cloned = me.clone();

    rayon::spawn(move || {
        let mut rng = thread_rng();
        // TODO don't hardcode this
        let params = FPTXParams::new(128, 4).unwrap();
        tx.send(
            Contribution::generate(&mut rng, maybe_previous.as_ref(), &me_cloned, &params)
        ).expect("Should never fail to send")
    });

    let my_contribution = rx.await?;
    handle.abort();

    // tell server we're done computing
    Msg::UpdateComputeProgress { finished: true, contributor: me.clone() }.sign(&my_sk).send().await?;

    let response = Msg::GetStatus { contributor: me.clone() }.sign(&my_sk).send_and_receive::<StatusResponse>().await?;
    let StatusResponse::ReadyForUpload(session_url) = response else {
        bail!("Finished compute, but server didn't give us the session url for upload");
    };

    // ping loop while uploading
    let me_cloned = me.clone();
    let my_sk_cloned = my_sk.clone();
    let handle = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(tokio::time::Duration::from_secs(5));
        loop {
            ticker.tick().await;
            Msg::UpdateUploadProgress { finished: false, contributor: me_cloned.clone() }.sign(&my_sk_cloned).send().await
                .expect("Should never fail to ping");
        }
    });

    const CHUNK_SIZE: usize = 64 * 1024 * 1024; // 8 MiB

    upload::upload_chunked(
        &session_url,
        &my_contribution,
        CHUNK_SIZE).await?;

    handle.abort();

    // tell server we're done uploading
    Msg::UpdateUploadProgress { finished: true, contributor: me.clone() }.sign(&my_sk).send().await?;


    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
    let mut response = Msg::GetStatus { contributor: me.clone() }.sign(&my_sk).send_and_receive::<StatusResponse>().await?;

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

        response = Msg::GetStatus { contributor: me.clone() }.sign(&my_sk).send_and_receive::<StatusResponse>().await?;
    }
}

