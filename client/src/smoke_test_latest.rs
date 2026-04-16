use std::{fs, path::Path};

use aptos_batch_encryption::{
    schemes::fptx_weighted::FPTXWeighted, shared::digest_key_file, tests::smoke::{fptx_weighted_smoke::run_pvss_with_hkzg, run_smoke} 
};
use aptos_crypto::weighted_config::WeightedConfigArkworks;
use common::{constants::CeremonyContribution, messages::Msg, truncate::Truncate};
use ed25519_dalek::SigningKey;
use anyhow::Result;



pub async fn smoke_test_latest(my_sk: &SigningKey, truncate: Option<usize>) -> Result<()> {
    let url : String = Msg::DownloadLatest.sign(&my_sk).send_and_receive().await?;

    eprintln!("{}: Downloading latest...", chrono::Local::now());
    let bytes = reqwest::get(url)
        .await?
        .bytes().await?;

    eprintln!("{}: Deserializing latest...", chrono::Local::now());
    let mut latest_contribution : CeremonyContribution = bcs::from_bytes(&bytes)?;


    eprintln!("Latest is from: {} and has hash {}", 
        latest_contribution.contributor().name, 
        latest_contribution.hash()
    );

    eprintln!("trace:");

    for (c, hash) in latest_contribution.previous_hashes() {
        eprintln!("{}: {}", c.name, hash.to_string());
    }

    if let Some(truncate_size) = truncate {
        eprintln!("{}: Truncating latest to {}", chrono::Local::now(), truncate_size);
        latest_contribution.truncate(truncate_size);
    }

    eprintln!("{}: Computing digest key and hkzg setup...", chrono::Local::now());
    let (dk, hkzg_setup) = latest_contribution.output();

    eprintln!("dk size: num rounds {}, max batch size {}", dk.num_rounds(), dk.max_batch_size());

    eprintln!("{}: Serializing dk...", chrono::Local::now());
    digest_key_file::write_digest_key(Path::new("digest_key.bin"), dk).unwrap();

    let dk = digest_key_file::read_digest_key(Path::new("digest_key.bin")).unwrap();

    eprintln!("{}: Running dummy DKG with HZKG setup...", chrono::Local::now());
    let tc = WeightedConfigArkworks::new(256, vec![1; 256]).unwrap();
    let (pp, tc, ek, vks, msk_shares) = run_pvss_with_hkzg(&dk, (hkzg_setup.1, hkzg_setup.0), &tc);

    eprintln!("{}: Running a batch encryption round...", chrono::Local::now());
    run_smoke::<FPTXWeighted>(tc, ek, dk, vks, msk_shares);
    eprintln!("{}: Succeeded!", chrono::Local::now());

    eprintln!("{}: Serializing pp...", chrono::Local::now());
    fs::write("pp.bin", &bcs::to_bytes(&pp).unwrap()).unwrap();

    Ok(())
}
