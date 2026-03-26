use std::sync::Arc;
use std::time::Duration;

use common::contribution::Contributor;
use anyhow::{Result, bail};
use gcp_auth::TokenProvider;
use google_cloud_auth::signer::Signer;
use google_cloud_storage::builder::storage::SignedUrlBuilder;
use google_cloud_storage::http::Method;

fn object_name(contributor: &Contributor) -> String {
    let hex_key = contributor
        .verifying_key
        .as_bytes()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    format!("contributions/{}.bin", hex_key)
}

/// Represents a contribution file in GCS.
#[derive(Debug)]
pub enum ContributionFileHandle {
    InProgress {
        contributor: Contributor,
        upload_session_url: String,
    },
    Complete {
        contributor: Contributor,
    },
}

impl ContributionFileHandle {
    pub fn url(&self, store: &ContributionFilesStore) -> String {
        let obj_name = object_name(self.contributor());
        format!(
            "{}/{}/{}",
            store.base_url,
            store.get_bucket_id(),
            obj_name
        )
    }

    fn contributor(&self) -> &Contributor {
        match self {
            ContributionFileHandle::InProgress { contributor, .. } => contributor,
            ContributionFileHandle::Complete { contributor } => contributor,
        }
    }

    pub fn should_be_finished(self) -> Result<Self> {
        match self {
            ContributionFileHandle::InProgress { .. } => {
                bail!("Expected contribution file to be finished, but got in progress")
            }
            ContributionFileHandle::Complete { .. } => Ok(self),
        }
    }

    pub fn should_not_be_finished(self) -> Result<Self> {
        match self {
            ContributionFileHandle::InProgress { .. } => Ok(self),
            ContributionFileHandle::Complete { .. } => {
                bail!("Expected contribution file to be in progress, but got a finished contribution")
            }
        }
    }

    pub async fn as_client_url(&self, store: &ContributionFilesStore) -> Result<String> {
        match self {
            ContributionFileHandle::InProgress { upload_session_url, .. } => {
                Ok(upload_session_url.clone())
            }
            ContributionFileHandle::Complete { contributor } => {
                let obj_name = object_name(contributor);
                store.generate_signed_download_url(&obj_name).await
            }
        }
    }
}

pub struct ContributionFilesStore {
    bucket_id: String,
    base_url: String,
    client: reqwest::Client,
    auth: Arc<dyn TokenProvider>,
    signer: Signer,
}

impl ContributionFilesStore {
    pub async fn init(bucket_id: &str) -> Result<Self> {
        let client = reqwest::Client::new();
        let auth = gcp_auth::provider().await?;
        let signer = google_cloud_auth::credentials::Builder::default().build_signer()?;
        Ok(Self {
            bucket_id: bucket_id.to_string(),
            base_url: "https://storage.googleapis.com".to_string(),
            client,
            auth,
            signer,
        })
    }

    pub fn get_bucket_id(&self) -> &str {
        &self.bucket_id
    }

    pub async fn get_or_create(&self, c: &Contributor) -> Result<ContributionFileHandle> {
        let obj_name = object_name(c);
        if self.object_exists(&obj_name).await? {
            Ok(ContributionFileHandle::Complete {
                contributor: c.clone(),
            })
        } else {
            let upload_session_url = self.initiate_resumable_upload(&obj_name).await?;
            Ok(ContributionFileHandle::InProgress {
                contributor: c.clone(),
                upload_session_url,
            })
        }
    }

    async fn get_token(&self) -> Result<String> {
        let scopes = &["https://www.googleapis.com/auth/devstorage.read_write"];
        let token = self.auth.token(scopes).await?;
        Ok(token.as_str().to_string())
    }

    async fn object_exists(&self, obj_name: &str) -> Result<bool> {
        let token = self.get_token().await?;
        let url = format!(
            "{}/storage/v1/b/{}/o/{}",
            self.base_url,
            urlencoding::encode(&self.bucket_id),
            urlencoding::encode(obj_name),
        );
        let resp = self.client.get(&url).bearer_auth(&token).send().await?;
        Ok(resp.status().is_success())
    }

    async fn initiate_resumable_upload(&self, obj_name: &str) -> Result<String> {
        let token = self.get_token().await?;
        let url = format!(
            "{}/upload/storage/v1/b/{}/o?uploadType=resumable&name={}",
            self.base_url,
            urlencoding::encode(&self.bucket_id),
            urlencoding::encode(obj_name),
        );
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&token)
            .header("Content-Length", "0")
            .send()
            .await?;

        let location = resp
            .headers()
            .get("Location")
            .ok_or_else(|| anyhow::anyhow!("Missing Location header in resumable upload response"))?
            .to_str()?
            .to_string();

        Ok(location)
    }

    pub async fn generate_signed_download_url(&self, obj_name: &str) -> Result<String> {
        let url = SignedUrlBuilder::for_object(&self.bucket_id, obj_name)
            .with_method(Method::GET)
            .with_expiration(Duration::from_secs(3600))
            .sign_with(&self.signer)
            .await?;
        Ok(url)
    }

    #[cfg(test)]
    fn new_with_base_url(bucket_id: &str, base_url: &str, auth: Arc<dyn TokenProvider>, signer: Signer) -> Self {
        Self {
            bucket_id: bucket_id.to_string(),
            base_url: base_url.to_string(),
            client: reqwest::Client::new(),
            auth,
            signer,
        }
    }
}

