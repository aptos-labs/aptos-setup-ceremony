use std::fs;

use aptos_batch_encryption::{
    group::G2Affine, schemes::fptx_weighted::FPTXWeighted, shared::digest::DigestKey, tests::smoke::{fptx_weighted_smoke::run_pvss_with_hkzg, run_smoke} 
};
use aptos_crypto::{TSecretSharingConfig as _, weighted_config::WeightedConfigArkworks};
use aptos_dkg::pvss::traits::TranscriptCore;
use common::{constants::CeremonyContribution, messages::Msg};
use ed25519_dalek::SigningKey;
use anyhow::Result;
use ark_ec::AffineRepr as _;


type T = aptos_dkg::pvss::chunky::SignedWeightedTranscript<aptos_batch_encryption::group::Pairing>;

pub async fn smoke_test_latest(my_sk: &SigningKey) -> Result<()> {
    let url : String = Msg::DownloadLatest.sign(&my_sk).send_and_receive().await?;

    eprintln!("{}: Downloading latest...", chrono::Local::now());
    let bytes = reqwest::get(url)
        .await?
        .bytes().await?;

    eprintln!("{}: Deserializing latest...", chrono::Local::now());
    let latest_contribution : CeremonyContribution = bcs::from_bytes(&bytes)?;

    eprintln!("Latest is from: {} and has hash {}", 
        latest_contribution.contributor().name, 
        latest_contribution.hash()
    );

    eprintln!("trace:");

    for (c, hash) in latest_contribution.previous_hashes() {
        eprintln!("{}: {}", c.name, hash.to_string());
    }



    eprintln!("{}: Computing digest key and hkzg setup...", chrono::Local::now());
    let (dk, hkzg_setup) = latest_contribution.output();

    eprintln!("dk size: {}, {}", dk.tau_powers_g1.len(), dk.tau_powers_g1[0].len());

    eprintln!("{}: Serializing dk...", chrono::Local::now());
    fs::write("dk.bin", &bitcode::serialize(&dk).unwrap()).unwrap();

    eprintln!("{}: Running dummy DKG with HZKG setup...", chrono::Local::now());
    let (pp, tc, ek, vks, msk_shares) = run_pvss_with_hkzg(&dk, (hkzg_setup.1, hkzg_setup.0));

    eprintln!("{}: Running a batch encryption round...", chrono::Local::now());
    run_smoke::<FPTXWeighted>(tc, ek, dk, vks, msk_shares);
    eprintln!("{}: Succeeded!", chrono::Local::now());

    eprintln!("{}: Serializing pp...", chrono::Local::now());
    fs::write("dk.bin", &bitcode::serialize(&pp).unwrap()).unwrap();

    Ok(())
}
