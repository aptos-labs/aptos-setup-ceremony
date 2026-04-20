use reqwest::Client;

pub async fn upload_parallel(
    part_urls: &[String],
    bytes: &[u8],
) -> anyhow::Result<()> {
    let total = bytes.len();
    let n = part_urls.len();
    assert!(n > 0, "upload_parallel called with zero part URLs");

    let base = total / n;
    let rem = total % n;
    let client = Client::new();

    let mut handles = Vec::with_capacity(n);
    let mut offset = 0usize;
    for (i, url) in part_urls.iter().enumerate() {
        let part_len = base + if i < rem { 1 } else { 0 };
        let slice = bytes[offset..offset + part_len].to_vec();
        offset += part_len;
        let url = url.clone();
        let client = client.clone();
        handles.push(tokio::spawn(async move {
            let resp = client
                .put(&url)
                .header("Content-Length", slice.len())
                .header("Content-Type", "application/octet-stream")
                .body(slice)
                .send()
                .await?;
            if !resp.status().is_success() {
                anyhow::bail!(
                    "Part {i} upload failed: {} - {}",
                    resp.status(),
                    resp.text().await.unwrap_or_default()
                );
            }
            anyhow::Ok(())
        }));
    }

    for h in handles {
        h.await??;
    }
    Ok(())
}
