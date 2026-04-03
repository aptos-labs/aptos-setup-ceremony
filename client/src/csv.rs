use anyhow::{Result, bail};
use common::contribution::{Contributor, AsAndFromHex};
use ed25519_dalek::{SigningKey, ed25519::signature::rand_core::CryptoRngCore};

pub enum KeypairsRow {
    NoKeypair {
        name: String,
        email: String,
    },
    HasKeypair {
        signing_key: SigningKey,
        contributor: Contributor
    }
}

impl KeypairsRow {
    pub fn must_have_keypair(self) 
    -> Result<(SigningKey, Contributor)> {
        match self {
            Self::HasKeypair { signing_key, contributor } => 
            Ok((signing_key, contributor)),
            Self::NoKeypair { .. } => 
            bail!("You haven't generated keypairs for all users yet")
        }
    }

    pub fn add_keypair_if_needed(
        self, 
        rng: &mut impl CryptoRngCore
    ) -> (SigningKey, Contributor) {
        match self {
            Self::HasKeypair { signing_key, contributor } => 
            (signing_key, contributor),
            Self::NoKeypair { name, email } => 
            Contributor::new(&name, &email, rng)
        }
    }
}


pub fn read_keypairs_file(file: &str) -> Result<Vec<KeypairsRow>> {
    let mut csv = csv::ReaderBuilder::new()
        .flexible(true)
        .from_path(file)?;
    csv.records().map(|maybe_row| {
        let row = maybe_row?;
        let name : String = row.get(0).ok_or(anyhow::anyhow!("Couldn't parse CSV"))?.to_string();
        let email : String = row.get(1).ok_or(anyhow::anyhow!("Couldn't parse CSV"))?.to_string();
        Ok(match row.get(2) {
            Some(keypair_hex) => {
                let (signing_key, contributor) = <(SigningKey, Contributor)>::from_hex(keypair_hex)?;

                if name != contributor.name {
                    bail!("Name mismatch");
                } else if email != contributor.email {
                    bail!("Email mismatch");
                }
                KeypairsRow::HasKeypair { signing_key, contributor }
            },
            None => {
                KeypairsRow::NoKeypair { name, email }
            }
        })
    }).collect()
}

pub fn write_keypairs_file(file: &str, keypairs: Vec<(SigningKey, Contributor)>) -> Result<()> {
    let mut writer = csv::Writer::from_path(file)?;
    writer.write_record(["Name", "Email", "Keypair"])?;
    for (sk, c) in keypairs {
        writer.write_record(
            [
                &c.name,
                &c.email,
                &(sk, c.clone()).as_hex()?
            ]
        )?;
    }

    Ok(())
}
