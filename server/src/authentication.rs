use common::contribution::Contributor;
use ed25519_dalek::{Signature, VerifyingKey, Verifier as _};
use serde::Serialize;
use crate::handlers::Config;
use anyhow::{Result, bail};



pub struct AuthenticatedMsg<Contents: Serialize> {
    pub inner: Contents,
    verifying_key: VerifyingKey,
    signature: Signature,
}

impl<Contents: Serialize> AuthenticatedMsg<Contents> {
    pub fn verify(&self) -> Result<()> {
        Ok(self.verifying_key.verify(&bcs::to_bytes(&self.inner)?, &self.signature)?)
    }

    pub fn verify_authenticated_by_admin(&self, config: &Config) -> Result<()> {
        if self.verifying_key != config.admin_verifying_key {
            bail!("Authentication failed: authorized by non-admin")
        } else {
            self.verify()
        }
    }
}

impl AuthenticatedMsg<Contributor> {
    pub fn verify_authenticated_by_contributor(&self) -> Result<()> {
        if self.verifying_key != self.inner.verifying_key {
            bail!("Authentication failed: authentication verifying key doesn't match contributor verifying key")
        } else {
            self.verify()
        }
    }
}

