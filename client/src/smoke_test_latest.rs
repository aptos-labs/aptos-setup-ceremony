use aptos_batch_encryption::{
    schemes::fptx_weighted::FPTXWeighted, tests::smoke::{fptx_weighted_smoke::run_pvss, run_smoke}, 
};
use common::{contribution::Contribution, fptx::FPTXContributionInner, messages::Msg};
use ed25519_dalek::SigningKey;
use anyhow::Result;

pub async fn smoke_test_latest(my_sk: &SigningKey) -> Result<()> {
    let url : String = Msg::DownloadLatest.sign(&my_sk).send_and_receive().await?;

    eprintln!("{}: Downloading latest...", chrono::Local::now());
    let bytes = reqwest::get(url)
        .await?
        .bytes().await?;

    eprintln!("{}: Deserializing latest...", chrono::Local::now());
    let latest_contribution : Contribution<FPTXContributionInner> = bcs::from_bytes(&bytes)?;

    eprintln!("Latest is from: {}", latest_contribution.contributor().name);

    eprintln!("trace:");

    for (c, _) in latest_contribution.previous_hashes() {
        eprintln!("{}", c.name);
    }

    eprintln!("{}: Computing digest key...", chrono::Local::now());
    let dk = latest_contribution.output();

    eprintln!("{}: Running dummy DKG...", chrono::Local::now());
    let (tc, ek, vks, msk_shares) = run_pvss(&dk);

    eprintln!("{}: Running smoke...", chrono::Local::now());
    run_smoke::<FPTXWeighted>(tc, ek, dk, vks, msk_shares);
    eprintln!("Succeeded!");

    Ok(())
}
