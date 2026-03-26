use common::{contribution::Contribution, fptx::FPTXContributionInner};
use reqwest::Client;

pub async fn upload_chunked(
    session_url: &str,
    data: &Contribution<FPTXContributionInner>,
    chunk_size: usize,
) -> anyhow::Result<()> {
        let client = Client::new();

    let bytes = bcs::to_bytes(data)?;
    let total = bytes.len();

    for (i, chunk) in bytes.chunks(chunk_size).enumerate() {
        let start = i * chunk_size;
        let end = (start + chunk.len()).saturating_sub(1);

        let resp = client
            .put(session_url)
            .header("Content-Length", chunk.len())
            .header(
                "Content-Range",
                format!("bytes {start}-{end}/{total}"),
            )
            .header("Content-Type", "application/octet-stream")
            .body(chunk.to_vec())
            .send()
            .await?;

        match resp.status().as_u16() {
            200 | 201 => {
                println!("Upload complete");
                break;
            }
            308 => {
                // Resume incomplete — GCS wants more chunks
                println!("Chunk {start}-{end}/{total} accepted");
            }
            _ => {
                anyhow::bail!(
                    "Upload failed at byte {start}: {} - {}",
                    resp.status(),
                    resp.text().await?
                );
            }
        }
    }

    Ok(())
}
