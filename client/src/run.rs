use std::path::PathBuf;
use std::fs;
use std::str::FromStr as _;

use common::contribution::{AsAndFromHex, Contributor};
use common::messages::Msg;
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

                let mut writer = csv::Writer::from_path("./report.csv")?;

                contributors.sort_by_key(|(p,row)| (row.status, *p));

                for (_, row) in contributors {
                    writer.serialize(row)?;
                }
                writer.flush()?;

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
            AdminCommand::SmokeTestLatest => {
                let (my_sk, _) = try_read_keypair_file(my_keypair_file)?;
                smoke_test_latest(&my_sk).await?;
            }
        }
    }

    Ok(())
}
