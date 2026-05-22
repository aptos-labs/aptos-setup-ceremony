use std::path::{Path, PathBuf};
use std::fs;
use std::str::FromStr as _;

use aptos_batch_encryption::schemes::fptx_weighted::FPTXWeighted;
use aptos_batch_encryption::shared::digest_key_file;
use aptos_batch_encryption::tests::smoke::SmokeTest;
use aptos_batch_encryption::tests::smoke::fptx_weighted_smoke::run_pvss_with_hkzg;
use aptos_crypto::weighted_config::WeightedConfigArkworks;
use common::aptos::{smoke_test_all_rounds, smoke_test_one_round};
use common::constants::{CeremonyContribution, PARAMS};
use common::contribution::{AsAndFromHex, Contributor};
use common::messages::Msg;
use common::truncate::Truncate;
use ed25519_dalek::SigningKey;
use rand::thread_rng;
use serde_json;
use anyhow::{self, Context as _, bail};
use server::handlers::ReportResponse;
use server::store::contributors_db::types::ContributorRow;

use crate::cli::{self, AdminCommand, Cli, Command};
use crate::contribute;
use crate::csv::{read_keypairs_file, write_keypairs_file};
use crate::smoke_test_latest::smoke_test_latest;



fn try_read_keypair_file(file: PathBuf) -> anyhow::Result<(SigningKey, Contributor)> {
    if !fs::exists(&file)? {
        bail!("Your keypair file does not exist. Please run `aptos-setup-ceremony identify`.");
    }

    Ok(serde_json::from_slice(&fs::read(file)?)?)
}

pub async fn run(cli: Cli, _config_dir: PathBuf) -> anyhow::Result<()> {
    let my_keypair_file = PathBuf::from_str(cli::KEYPAIR_FILE)?;

    match cli.command {
        Command::GenerateKeypair { name, email, force} => {
            if fs::exists(&my_keypair_file)? && !force {
                bail!("Your keypair already exists at {:?}. Please delete it or use --force to overwrite.", my_keypair_file);
            }
let mut rng = thread_rng();
            let me = Contributor::new(&name, &email, &mut rng);
            let keypair_json = serde_json::to_string(&me)?;
            fs::write(&my_keypair_file, keypair_json)?;
            eprintln!("Your verifying key is {}", me.1.verifying_key.as_hex()?);
            eprintln!("Keypair file written to {:?}", my_keypair_file);
        },
        Command::Identify { contributor_keypair_hex, force } => {
            if fs::exists(&my_keypair_file)? && !force {
                bail!("Your keypair already exists at {:?}. Please delete it or use --force to overwrite.", my_keypair_file);
            }
            let (my_sk, me) = &<(SigningKey, Contributor)>::from_hex(&contributor_keypair_hex)
                .context("Couldn't parse the keypair hex. Please make sure you copied it correctly.")?;
            let keypair_json = serde_json::to_string(&(my_sk, me))?;
            fs::write(&my_keypair_file, keypair_json)?;
            eprintln!("You are contributing as {}.", me.name);
            eprintln!("Keypair file written to {:?}", my_keypair_file);
        },
        Command::Contribute => {
            let (my_sk, me) = try_read_keypair_file(my_keypair_file)?;

            contribute::contribute(my_sk, &me).await?;
        },
        Command::Verify { current_file, previous_file } => {
            eprintln!("{}: Reading current file...", chrono::Local::now());
            let current_bytes = fs::read(&current_file)?;
            eprintln!("{}: Computing hash for current file...", chrono::Local::now());
            let current_hash = blake3::hash(&current_bytes);
            eprintln!("current hash: {}", current_hash);
            eprintln!("{}: Deserializing current file...", chrono::Local::now());
            let current : CeremonyContribution = bcs::from_bytes(&current_bytes)?;

            let previous : Option<CeremonyContribution> = match previous_file {
                None => None,
                Some(previous_file) => {
                    eprintln!("{}: Reading previous file...", chrono::Local::now());
                    let previous_bytes = fs::read(&previous_file)?;
                    eprintln!("{}: Computing hash for previous file...", chrono::Local::now());
                    let previous_hash = blake3::hash(&previous_bytes);
                    eprintln!("previous hash: {}", previous_hash);
                    eprintln!("{}: Deserializing previous file...", chrono::Local::now());
                    let previous : CeremonyContribution = bcs::from_bytes(&previous_bytes)?;
                    Some(previous)
                }
            };

            eprintln!("{}: Verifying...", chrono::Local::now());
            let mut rng = thread_rng();
            current.verify(&mut rng, previous.as_ref(), &PARAMS)?;
            eprintln!("{}: Succeeded.", chrono::Local::now());

        },
        Command::ComputeOutput { contribution_file, truncate, smoke_test_one, smoke_test_all } => {
            eprintln!("{}: Reading first file...", chrono::Local::now());
            let contribution_bytes = fs::read(&contribution_file)?;
            eprintln!("{}: Computing hash for first file...", chrono::Local::now());
            let contribution_hash = blake3::hash(&contribution_bytes);
            eprintln!("contribution hash: {}", contribution_hash);
            let mut contribution : CeremonyContribution = bcs::from_bytes(&contribution_bytes)?;

            if let Some(truncate_size) = truncate {
                eprintln!("{}: Truncating contribution to {}", chrono::Local::now(), truncate_size);
                contribution.truncate(truncate_size);
            }

            eprintln!("{}: Computing digest key and hkzg setup...", chrono::Local::now());
            let (dk, hkzg_setup) = contribution.output();

            eprintln!("dk size: num rounds {}, max batch size {}", dk.num_rounds(), dk.max_batch_size());

            eprintln!("{}: Computing pp...", chrono::Local::now());
            let tc = WeightedConfigArkworks::new(256, vec![1; 256]).unwrap();
            let (pp, tc, ek, vks, msk_shares) = run_pvss_with_hkzg(&dk, (hkzg_setup.1, hkzg_setup.0), &tc);


            eprintln!("{}: Serializing dk...", chrono::Local::now());
            digest_key_file::write_digest_key(Path::new("digest_key.bin"), dk)?;



            eprintln!("{}: Serializing pp...", chrono::Local::now());
            fs::write("pp.bin", &bcs::to_bytes(&pp)?)?;

            if smoke_test_all {
                eprintln!("{}: Batch encryption smoke test...", chrono::Local::now());
                let dk = digest_key_file::read_digest_key(Path::new("digest_key.bin")).unwrap();
                let num_rounds = dk.num_rounds();
                let smoke_test = SmokeTest::<FPTXWeighted>::new(tc, ek, dk, vks, msk_shares);

                smoke_test_all_rounds(&smoke_test, num_rounds);
            } else if smoke_test_one { 
                eprintln!("{}: Batch encryption smoke test...", chrono::Local::now());
                let dk = digest_key_file::read_digest_key(Path::new("digest_key.bin")).unwrap();
                let smoke_test = SmokeTest::<FPTXWeighted>::new(tc, ek, dk, vks, msk_shares);

                smoke_test_one_round(&smoke_test);
            }

        },
        Command::Admin { command } => match command {
            AdminCommand::GenerateAllKeypairs { file } => {
                let mut rng = thread_rng();
                let keypairs = read_keypairs_file(&file)?
                    .into_iter()
                    .map(|row| row.add_keypair_if_needed(&mut rng))
                    .collect();
                write_keypairs_file(&(file), keypairs)?;
            }
            AdminCommand::RegisterAll { file } => {
                let (my_sk, _) = try_read_keypair_file(my_keypair_file)?;
                let keypairs : Vec<(SigningKey, Contributor)> 
                = read_keypairs_file(&file)?
                    .into_iter()
                    .map(|row| row.must_have_keypair())
                    .collect::<anyhow::Result<Vec<(SigningKey, Contributor)>>>()?;

                for (_, contributor) in keypairs {
                    let result = Msg::Register { contributor: contributor.clone() }.sign(&my_sk).send().await;
                    match result {
                        Ok(_) => eprintln!("{} registered", contributor.name),
                        Err(e) => eprintln!("{} failed to register, maybe already exists? {:?}", contributor.name, e),
                    }
                }
            },
            AdminCommand::Report => {
                let (my_sk, _) = try_read_keypair_file(my_keypair_file)?;

                let ReportResponse { status, mut contributors } = Msg::Report.sign(&my_sk).send_and_receive::<ReportResponse>().await?;

                //let mut writer = csv::Writer::from_path("./report.csv")?;

                contributors.sort_by_key(|(p,row)| (row.status, *p));

                for (_, row) in contributors {
                    println!("{}: {:?}, last updated: {}", row.name, row.get_current_contribution_step(), row.updated_timestamp);
                    //writer.serialize(row)?;
                }
                //writer.flush()?;

                println!("Current status:");
                println!("{:?}", status);


            }
            AdminCommand::DownloadAll => {
                let (my_sk, _) = try_read_keypair_file(my_keypair_file)?;

                let finished : Vec<(ContributorRow, String)> = Msg::DownloadAll.sign(&my_sk).send_and_receive().await?;

                // Don't need to sort b/c already sorted

                eprintln!("Downloading all contributions...");


                for (i, (c, url)) in finished.into_iter().enumerate() {
                    let bytes = reqwest::get(url)
                        .await?
                        .bytes().await?;

                    let name_no_space = c.name.replace(" ", "-");

                    fs::write(format!("{:03}-{}-{}.contrib", i+1, c.verifying_key.as_ref().as_hex()?, name_no_space), bytes)?;
                }
            },
            AdminCommand::SmokeTestLatest { truncate, all } => {
                let (my_sk, _) = try_read_keypair_file(my_keypair_file)?;
                smoke_test_latest(&my_sk, truncate, all).await?;
            }
        }
    }

    Ok(())
}
