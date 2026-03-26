use std::path::PathBuf;
use std::fs;

use common::contribution::{Contributor, AsAndFromHex};
use ed25519_dalek::SigningKey;
use rand::thread_rng;
use serde_json;
use anyhow;
use tabled::{Table, Tabled};
use hex;

use crate::cli::{self, AdminCommand, Cli, Command};
use crate::csv::{read_keypairs_file, read_users_file, write_keypairs_file};


#[derive(Tabled)]
struct TabledKeypair {
    #[tabled(inline)]
    contributor: Contributor,
    #[tabled(format("{:?}", hex::encode(self.signing_key.as_bytes())))]
    signing_key: SigningKey,


}

pub fn run(cli: Cli, config_dir: PathBuf) -> anyhow::Result<()> {
    let keypair_file = config_dir.join(cli::KEYPAIR_FILE);

    match cli.command {
        Command::GenerateKeypair { name, email, force} => {
            if fs::exists(&keypair_file)? && !force {
                eprintln!("Your keypair already exists at {:?}. Please delete it or use --force to overwrite.", keypair_file);
                return Ok(());
            }
            let mut rng = thread_rng();
            let keypair_json = serde_json::to_string(&Contributor::new(&name, &email, &mut rng))?;
            fs::write(&keypair_file, keypair_json)?;
            eprintln!("Keypair file written to {:?}", keypair_file);
        },
        Command::Identify { contributor_keypair_hex, force } => {
            if fs::exists(&keypair_file)? && !force {
                eprintln!("Your keypair already exists at {:?}. Please delete it or use --force to overwrite.", keypair_file);
                return Ok(());
            }
            let keypair_json = serde_json::to_string(&<(SigningKey, Contributor)>::from_hex(&contributor_keypair_hex)?)?;
            fs::write(&keypair_file, keypair_json)?;
            eprintln!("Keypair file written to {:?}", keypair_file);
        },
        Command::Contribute => todo!(),
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
                let _keypairs = read_keypairs_file(&file)?;

                
            },
            AdminCommand::Report => todo!(),
            AdminCommand::DownloadAll => todo!(),
        }
    }

    Ok(())
}
