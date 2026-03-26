use anyhow::{Result, bail};
use common::contribution::{Contributor, AsAndFromHex};
use ed25519_dalek::SigningKey;

pub fn read_users_file(file: &str) -> Result<Vec<(String, String)>> {
    let mut csv = csv::Reader::from_path(file)?;
    csv.records().map(|maybe_row| {
        let row = maybe_row?;
        Ok((
            String::from(row.get(0).ok_or(anyhow::anyhow!("Couldn't parse CSV"))?), 
            String::from(row.get(1).ok_or(anyhow::anyhow!("Couldn't parse CSV"))?),  
        ))
    }).collect()
}


pub fn read_keypairs_file(file: &str) -> Result<Vec<(SigningKey, Contributor)>> {
    let mut csv = csv::Reader::from_path(file)?;
    csv.records().map(|maybe_row| {
        let row = maybe_row?;
        let name = row.get(0).ok_or(anyhow::anyhow!("Couldn't parse CSV"))?;
        let email = row.get(1).ok_or(anyhow::anyhow!("Couldn't parse CSV"))?;
        let keypair_hex = row.get(2).ok_or(anyhow::anyhow!("Couldn't parse CSV"))?;
        let (sk, c) = <(SigningKey, Contributor)>::from_hex(keypair_hex)?;

        if name != c.name {
            bail!("Name mismatch")
        } else if email != c.email {
            bail!("Email mismatch")
        } else {
            Ok((sk, c))
        }
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
