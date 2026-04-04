use aptos_batch_encryption::{
    schemes::fptx_weighted::FPTXWeighted, tests::smoke::{fptx_weighted_smoke::{run_pvss_with_hkzg}, run_smoke}, 
};
use common::{constants::CeremonyContribution, messages::Msg};
use ed25519_dalek::SigningKey;
use anyhow::Result;

pub async fn smoke_test_latest(my_sk: &SigningKey) -> Result<()> {
    let url : String = Msg::DownloadLatest.sign(&my_sk).send_and_receive().await?;

    eprintln!("{}: Downloading latest...", chrono::Local::now());
    let bytes = reqwest::get(url)
        .await?
        .bytes().await?;

    eprintln!("{}: Deserializing latest...", chrono::Local::now());
    let latest_contribution : CeremonyContribution = bcs::from_bytes(&bytes)?;

    eprintln!("Latest is from: {}", latest_contribution.contributor().name);

    eprintln!("trace:");

    for (c, hash) in latest_contribution.previous_hashes() {
        eprintln!("{}: {}", c.name, hash.to_string());
    }

    eprintln!("{}: Computing digest key and hkzg setup...", chrono::Local::now());
    let (dk, hkzg_setup) = latest_contribution.output();

    eprintln!("{}: Running dummy DKG with HZKG setup...", chrono::Local::now());
    let (tc, ek, vks, msk_shares) = run_pvss_with_hkzg(&dk, (hkzg_setup.1, hkzg_setup.0));


    eprintln!("{}: Running a batch encryption round...", chrono::Local::now());
    run_smoke::<FPTXWeighted>(tc, ek, dk, vks, msk_shares);
    eprintln!("Succeeded!");

    Ok(())
}
