use chrono::TimeDelta;
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Deserializer};


fn deserialize_verifying_key<'de, D: Deserializer<'de>>(deserializer: D) -> Result<VerifyingKey, D::Error> {
    let hex_str = String::deserialize(deserializer)?;
    let bytes: Vec<u8> = (0..hex_str.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex_str[i..i + 2], 16))
        .collect::<Result<Vec<u8>, _>>()
        .map_err(serde::de::Error::custom)?;
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| serde::de::Error::custom("verifying key must be 32 bytes"))?;
    VerifyingKey::from_bytes(&bytes).map_err(serde::de::Error::custom)
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
fn default_download_timeout() -> i64 { 1 }
fn default_contribute_timeout() -> i64 { 1500 }
fn default_upload_timeout() -> i64 { 600 }
fn default_port() -> u16 { 8888 }

impl Config {
    pub fn ping_timeout(&self) -> TimeDelta { TimeDelta::seconds(self.ping_timeout_secs) }
    pub fn download_timeout(&self) -> TimeDelta { TimeDelta::seconds(self.download_timeout_secs) }
    pub fn contribute_timeout(&self) -> TimeDelta { TimeDelta::seconds(self.contribute_timeout_secs) }
    pub fn upload_timeout(&self) -> TimeDelta { TimeDelta::seconds(self.upload_timeout_secs) }
}

