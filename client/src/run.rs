use std::path::PathBuf;
use std::fs;
use std::str::FromStr as _;

use common::contribution::{AsAndFromHex, Contribution, Contributor};
use common::fptx::FPTXContributionInner;
use common::messages::Msg;
use ed25519_dalek::SigningKey;
use rand::{Rng as _, thread_rng};
use serde_json;
use anyhow::{self, bail};
use server::handlers::ReportResponse;
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
            AdminCommand::DownloadAll => todo!(),
            AdminCommand::SanityTestLatest => {
                use aptos_batch_encryption::{
                    schemes::fptx_weighted::FPTXWeighted, tests::smoke::run_smoke, traits::BatchThresholdEncryption as _,
                };
                use aptos_crypto::weighted_config::WeightedConfigArkworks;

                let (my_sk, _) = try_read_keypair_file(my_keypair_file)?;

                let url : String = Msg::DownloadLatest.sign(&my_sk).send_and_receive().await?;

                eprintln!("Downloading latest...");
                let bytes = reqwest::get(url)
                    .await?
                    .bytes().await?;

                eprintln!("Deserializing latest...");
                let latest_contribution : Contribution<FPTXContributionInner> = bcs::from_bytes(&bytes)?;

                eprintln!("Initializing FPTX params...");
                let tc = WeightedConfigArkworks::new(3, vec![1, 2, 5]).unwrap();

                let (mut ek, _, vks, msk_shares) =
                FPTXWeighted::setup_for_testing(thread_rng().r#gen(), 8, 1, &tc).unwrap();

                let dk = latest_contribution.output();
                ek.use_digest_key(&dk);

                eprintln!("Running smoke...");
                run_smoke::<FPTXWeighted>(tc, ek, dk, vks, msk_shares);
                eprintln!("Succeeded!");
            }
        }
    }

    Ok(())
}
