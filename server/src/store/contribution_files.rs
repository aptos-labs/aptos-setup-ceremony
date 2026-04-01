use std::sync::Arc;
use bytes::Bytes;
use std::time::Duration;

use common::contribution::Contributor;
use anyhow::{Result, bail};
use gcp_auth::TokenProvider;
use google_cloud_auth::signer::Signer;
use google_cloud_gax::error::rpc::Code;
use google_cloud_storage::builder::storage::SignedUrlBuilder;
use google_cloud_storage::client::{Storage, StorageControl};
use google_cloud_storage::http::Method;
use google_cloud_storage::model::Bucket;

fn object_name(contributor: &Contributor) -> String {
    let hex_key = contributor
        .verifying_key
        .as_bytes()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    format!("contributions/{}.bin", hex_key)
}

pub struct ContributionFilesStore {
    bucket_id: String,
    project_id: String,
    base_url: String,
    client: reqwest::Client,
    auth: Arc<dyn TokenProvider>,
    signer: Signer,
    pub(crate) gcs_client: Storage,
    control_client: StorageControl,
}

impl ContributionFilesStore {
    pub async fn init(project_id: &str, bucket_id: &str) -> Result<Self> {
        let client = reqwest::Client::new();
        let auth = gcp_auth::provider().await?;
        let signer = google_cloud_auth::credentials::Builder::default().build_signer()?;
        let gcs_client = Storage::builder().build().await?;
        let control_client = StorageControl::builder().build().await?;
        Ok(Self {
            bucket_id: bucket_id.to_string(),
            project_id: project_id.to_string(),
            base_url: "https://storage.googleapis.com".to_string(),
            client,
            auth,
            signer,
            gcs_client,
            control_client,
        })
    }

    pub fn get_bucket_id(&self) -> &str {
        &self.bucket_id
    }

    pub async fn ensure_bucket_exists(&self) -> Result<()> {
        let name = format!("projects/_/buckets/{}", self.bucket_id);
        match self.control_client.get_bucket().set_name(name).send().await {
            Ok(_) => Ok(()),
            Err(e) if e.status().is_some_and(|s| s.code == Code::NotFound) => {
                let mut bucket = Bucket::default();
                bucket.project = format!("projects/{}", self.project_id);
                match self.control_client
                    .create_bucket()
                    .set_parent("projects/_")
                    .set_bucket_id(self.bucket_id.clone())
                    .set_bucket(bucket)
                    .send()
                    .await
                {
                    Ok(_) => Ok(()),
                    // Lost a creation race — bucket was created between our get and create
                    Err(e) if e.status().is_some_and(|s| s.code == Code::AlreadyExists) => Ok(()),
                    Err(e) => Err(e.into()),
                }
            }
            Err(e) => Err(e.into()),
        }
    }

    pub async fn get_download_url(&self, c: &Contributor) -> Result<String> {
        let obj_name = object_name(c);
        if self.object_exists(&obj_name).await? {
            self.generate_signed_download_url(&obj_name).await
        } else {
            bail!("Contributor's file does not exist")
        }
    }

    pub async fn get_upload_url(&self, c: &Contributor) -> Result<String> {
        let obj_name = object_name(c);
        if self.object_exists(&obj_name).await? {
            self.delete_object(&obj_name).await?;
        } 
        self.initiate_resumable_upload(&obj_name).await
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

    /// Deletes a GCS object; used for test cleanup.
    async fn delete_object(&self, obj_name: &str) -> Result<()> {
        let auth = gcp_auth::provider().await.unwrap();
        let token = auth
            .token(&["https://www.googleapis.com/auth/devstorage.read_write"])
            .await
            .unwrap();
        let url = format!(
            "{}/storage/v1/b/{}/o/{}",
            self.base_url,
            urlencoding::encode(&self.bucket_id),
            urlencoding::encode(obj_name),
        );
        let resp = self.client
            .delete(&url)
            .bearer_auth(token.as_str())
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!(
                "GCS deletion failed with status {}: {}",
                status,
                body,
            );
        }


        Ok(())
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

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!(
                "GCS resumable upload initiation failed with status {}: {}",
                status,
                body,
            );
        }

        let location = resp
            .headers()
            .get("Location")
            .ok_or_else(|| anyhow::anyhow!("Missing Location header in resumable upload response"))?
            .to_str()?
            .to_string();

        Ok(location)
    }

    pub async fn download_contribution<T: serde::de::DeserializeOwned>(
        &self,
        contributor: &Contributor,
    ) -> Result<T> {
        let obj_name = object_name(contributor);
        let bucket = format!("projects/_/buckets/{}", self.bucket_id);
        let mut reader = self.gcs_client
            .read_object(&bucket, &obj_name)
            .send()
            .await?;
        let mut bytes = Vec::new();
        while let Some(chunk) = reader.next().await.transpose()? {
            bytes.extend_from_slice(&chunk);
        }
        Ok(bcs::from_bytes(&bytes)?)
    }

    pub async fn generate_signed_download_url(&self, obj_name: &str) -> Result<String> {
        let bucket = format!("projects/_/buckets/{}", self.bucket_id);
        let url = SignedUrlBuilder::for_object(&bucket, obj_name)
            .with_method(Method::GET)
            .with_expiration(Duration::from_secs(3600))
            .sign_with(&self.signer)
            .await?;
        Ok(url)
    }

    pub async fn write_test_blob(&self, blob: Bytes) -> Result<()> {
        let bucket = format!("projects/_/buckets/{}", self.bucket_id);
        self.gcs_client
            .write_object(bucket, "test_download_blob", blob)
            .send_unbuffered()
        .await?;
        
        Ok(())
    }

    pub async fn get_test_blob_download_url(&self) -> Result<String> {
        self.generate_signed_download_url("test_download_blob").await
    }
}

