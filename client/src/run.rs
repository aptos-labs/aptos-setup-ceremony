use std::path::PathBuf;
use std::fs;
use std::str::FromStr as _;

use aptos_batch_encryption::tests::smoke::fptx_weighted_smoke::run_pvss;
use common::contribution::{AsAndFromHex, Contribution, Contributor};
use common::fptx::FPTXContributionInner;
use common::messages::Msg;
use ed25519_dalek::SigningKey;
use rand::{Rng as _, thread_rng};
use serde_json;
use anyhow::{self, bail};
use server::handlers::ReportResponse;
use server::store::contributors_db::ContributorState;
use tabled::{Table, Tabled};
use hex;

use crate::cli::{self, AdminCommand, Cli, Command};
use crate::csv::{read_keypairs_file, read_users_file, write_keypairs_file};
use client::contribute;


#[derive(Tabled)]
struct TabledKeypair {
    #[tabled(inline)]
    contributor: Contributor,
    #[tabled(format("{:?}", hex::encode(self.signing_key.as_bytes())))]
    signing_key: SigningKey,
}


fn try_read_keypair_file(file: PathBuf) -> anyhow::Result<(SigningKey, Contributor)> {
    if !fs::exists(&file)? {
        bail!("Your keypair file does not exist. Please run `aptos-setup-ceremony identify`.");
    }

    Ok(serde_json::from_slice(&fs::read(file)?)?)
}

pub async fn run(cli: Cli, _config_dir: PathBuf) -> anyhow::Result<()> {
    // TODO change this back for prod?
    //let my_keypair_file = config_dir.join(cli::KEYPAIR_FILE);
    let my_keypair_file = PathBuf::from_str(cli::KEYPAIR_FILE)?;

    match cli.command {
        Command::GenerateKeypair { name, email, force} => {
            if fs::exists(&my_keypair_file)? && !force {
                bail!("Your keypair already exists at {:?}. Please delete it or use --force to overwrite.", my_keypair_file);
            }
            let mut rng = thread_rng();
            let keypair_json = serde_json::to_string(&Contributor::new(&name, &email, &mut rng))?;
            fs::write(&my_keypair_file, keypair_json)?;
            eprintln!("Keypair file written to {:?}", my_keypair_file);
        },
        Command::Identify { contributor_keypair_hex, force } => {
            if fs::exists(&my_keypair_file)? && !force {
                bail!("Your keypair already exists at {:?}. Please delete it or use --force to overwrite.", my_keypair_file);
            }
            let (my_sk, me) = &<(SigningKey, Contributor)>::from_hex(&contributor_keypair_hex)?;
            let keypair_json = serde_json::to_string(&(my_sk, me))?;
            fs::write(&my_keypair_file, keypair_json)?;
            eprintln!("You are contributing as {}.", me.name);
            eprintln!("Keypair file written to {:?}", my_keypair_file);
        },
        Command::Contribute => {
            let (my_sk, me) = try_read_keypair_file(my_keypair_file)?;

            contribute::contribute(my_sk, &me).await?;
        },
        Command::Admin { command } => match command {
            AdminCommand::GenerateAllKeypairs { file } => {
                let mut rng = thread_rng();
                let keypairs = read_users_file(&file)?
                    .into_iter()
                    .map(|(name, email)| Contributor::new(&name, &email, &mut rng))
                    .collect();

                write_keypairs_file(&(file+".keypairs"), keypairs)?;
            }
            AdminCommand::ReadKeypairsFile { file } => {
                let keypairs : Vec<TabledKeypair> = read_keypairs_file(&file)?
                    .into_iter()
                    .map(|(signing_key, contributor)| TabledKeypair { signing_key, contributor })
                    .collect();
                let table = Table::new(keypairs).to_string();
                println!("{table}");
            }
            AdminCommand::RegisterAll { file } => {
                let (my_sk, _) = try_read_keypair_file(my_keypair_file)?;
                let keypairs = read_keypairs_file(&file)?;

                for (_, contributor) in keypairs {
                        Msg::Register { contributor }.sign(&my_sk).send().await?;
                }
            },
            AdminCommand::Report => {
                let (my_sk, _) = try_read_keypair_file(my_keypair_file)?;

                let ReportResponse { status, contributors } = Msg::Report.sign(&my_sk).send_and_receive::<ReportResponse>().await?;

                let table = Table::new(contributors).to_string();
                println!("{table}");

                println!("Current status:");
                let table = Table::new(Vec::from([status])).to_string();
                println!("{table}");


            }
            AdminCommand::DownloadAll => {
                let (my_sk, _) = try_read_keypair_file(my_keypair_file)?;

                let mut finished : Vec<(ContributorState, String)> = Msg::DownloadAll.sign(&my_sk).send_and_receive().await?;

                // Don't need to sort b/c already sorted

                eprintln!("Downloading all contributions...");


                for (i, (c, url)) in finished.into_iter().enumerate() {
                    let bytes = reqwest::get(url)
                        .await?
                        .bytes().await?;

                    let name_no_space = c.contributor.name.replace(" ", "-");

                    fs::write(format!("{:03}-{}-{}.contrib", i+1, c.contributor.verifying_key.as_hex()?, name_no_space), bytes)?;
                }
            },
            AdminCommand::SmokeTestLatest => {
                use aptos_batch_encryption::{
                    schemes::fptx_weighted::FPTXWeighted, tests::smoke::run_smoke, 
                };

                let (my_sk, _) = try_read_keypair_file(my_keypair_file)?;

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
            }
        }
    }

    Ok(())
}
