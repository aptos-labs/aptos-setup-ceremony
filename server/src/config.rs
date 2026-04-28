use chrono::TimeDelta;
use common::contribution::AsAndFromHex;
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Deserializer};


fn deserialize_verifying_key<'de, D: Deserializer<'de>>(deserializer: D) -> Result<VerifyingKey, D::Error> {
    let hex_str = String::deserialize(deserializer)?;
    Ok(VerifyingKey::from_hex(&hex_str)
        .map_err(|e| 
            serde::de::Error::custom(format!("{:?}", e))
        )?
    )
}


#[derive(Deserialize, Clone)]
pub struct Config {
    pub db_path: String,
    pub bucket_id: String,
    pub gcp_project_id: String,
    #[serde(deserialize_with = "deserialize_verifying_key")]
    pub admin_verifying_key: VerifyingKey,
    #[serde(default = "default_ping_timeout")]
    pub ping_timeout_secs: i64,
    #[serde(default = "default_download_timeout")]
    pub download_timeout_secs: i64,
    #[serde(default = "default_contribute_timeout")]
    pub contribute_timeout_secs: i64,
    #[serde(default = "default_upload_timeout")]
    pub upload_timeout_secs: i64,
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_ping_timeout() -> i64 { 20 }
fn default_download_timeout() -> i64 { 90 }
fn default_contribute_timeout() -> i64 { 900 }
fn default_upload_timeout() -> i64 { 350 }
fn default_port() -> u16 { 8888 }

impl Config {
    pub fn ping_timeout(&self) -> TimeDelta { TimeDelta::seconds(self.ping_timeout_secs) }
    pub fn download_timeout(&self) -> TimeDelta { TimeDelta::seconds(self.download_timeout_secs) }
    pub fn contribute_timeout(&self) -> TimeDelta { TimeDelta::seconds(self.contribute_timeout_secs) }
    pub fn upload_timeout(&self) -> TimeDelta { TimeDelta::seconds(self.upload_timeout_secs) }
}

