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

    pub async fn create_or_overwrite(&self, c: &Contributor) -> Result<ContributionFileHandle> {
        let obj_name = object_name(c);
        if self.object_exists(&obj_name).await? {
            self.delete_object(&obj_name).await?;
        } 
        let upload_session_url = self.initiate_resumable_upload(&obj_name).await?;
        Ok(ContributionFileHandle::InProgress {
            contributor: c.clone(),
            upload_session_url,
        })

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

#[cfg(test)]
mod tests {
    use std::env;
    use rand::thread_rng;
    use common::contribution::Contributor;
    use super::*;

    const PROJECT_ID: &str = "benchmark-zkid-circuit";
    const TEST_BUCKET: &str = "benchmark-zkid-circuit-test";

    fn setup() {
        // rustls 0.23 requires an explicit crypto provider to be installed once per process.
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let home = env::var("HOME").unwrap();
        // SAFETY: single-threaded test setup, no concurrent env reads
        unsafe {
            env::set_var(
                "GOOGLE_APPLICATION_CREDENTIALS",
                format!("{}/test-gcs-server.json", home),
            );
        }
    }


    /// Uploads all `bytes` to a GCS resumable upload session URL, finalising the object.
    async fn finalize_resumable_upload(session_url: &str, bytes: Vec<u8>) {
        let last = bytes.len() - 1;
        let total = bytes.len();
        let resp = reqwest::Client::new()
            .put(session_url)
            .header("Content-Length", total.to_string())
            .header("Content-Range", format!("bytes 0-{}/{}", last, total))
            .body(bytes)
            .send()
            .await
            .unwrap();
        assert!(
            resp.status().is_success(),
            "resumable upload failed with status {}",
            resp.status()
        );
    }

    #[tokio::test]
    async fn test_contribution_files_store() {
        setup();

        let mut rng = thread_rng();
        let (_, contributor) = Contributor::new("Integration Test", "test@example.com", &mut rng);

        let store = ContributionFilesStore::init(PROJECT_ID, TEST_BUCKET).await.unwrap();

        // --- ensure_bucket_exists is idempotent ---
        store.ensure_bucket_exists().await.unwrap();
        store.ensure_bucket_exists().await.unwrap();

        // --- get_or_create on a brand-new contributor → InProgress ---
        let handle = store.get_or_create(&contributor).await.unwrap();
        let upload_session_url = match &handle {
            ContributionFileHandle::InProgress { upload_session_url, .. } => upload_session_url.clone(),
            ContributionFileHandle::Complete { .. } => panic!("expected InProgress for new contributor"),
        };

        // url() should include bucket and object path
        let obj_url = handle.url(&store);
        assert!(obj_url.contains(TEST_BUCKET), "url missing bucket: {}", obj_url);
        assert!(obj_url.contains("contributions/"), "url missing object path: {}", obj_url);

        // as_client_url on InProgress returns the upload session URL unchanged
        let client_url = handle.as_client_url(&store).await.unwrap();
        assert_eq!(client_url, upload_session_url);

        // should_not_be_finished passes for InProgress
        handle.should_not_be_finished().unwrap();

        // --- Upload the file via the resumable session URL ---
        let payload: Vec<u8> = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let encoded = bcs::to_bytes(&payload).unwrap();
        finalize_resumable_upload(&upload_session_url, encoded).await;

        // --- get_or_create after upload → Complete ---
        let handle = store.get_or_create(&contributor).await.unwrap();
        assert!(
            matches!(handle, ContributionFileHandle::Complete { .. }),
            "expected Complete after upload"
        );

        // should_be_finished passes for Complete
        handle.should_be_finished().unwrap();

        // --- download_contribution round-trips the BCS payload ---
        let downloaded: Vec<u8> = store.download_contribution(&contributor).await.unwrap();
        assert_eq!(downloaded, payload);

        // --- generate_signed_download_url returns an https URL ---
        let obj_name = object_name(&contributor);
        let signed_url = store.generate_signed_download_url(&obj_name).await.unwrap();
        assert!(signed_url.starts_with("https://"), "signed URL should be https: {}", signed_url);

        // --- as_client_url on Complete returns a signed download URL ---
        let handle = store.get_or_create(&contributor).await.unwrap();
        let client_url = handle.as_client_url(&store).await.unwrap();
        assert!(client_url.starts_with("https://"), "client URL should be https: {}", client_url);

        // --- Cleanup ---
        store.delete_object(&obj_name).await.unwrap();
    }
}

