use chrono::{DateTime, Utc};
use common::contribution::Contributor;
use ed25519_dalek::VerifyingKey;

use anyhow::{Context, Result, bail};
use sqlx::SqlitePool;

#[derive(Eq, PartialEq)]
pub enum Status {
    WaitingForDownload {
        start: DateTime<Utc>,
    },
    WaitingForCompute {
        start: DateTime<Utc>,
    },
    WaitingForUpload {
        start: DateTime<Utc>,
    },
    Verifying {
        start: DateTime<Utc>,
    },
}

impl Status {
    pub fn start(&self) -> &DateTime<Utc> {
        match self {
            Status::WaitingForDownload { start } => start,
            Status::WaitingForCompute { start } => start,
            Status::WaitingForUpload { start } => start,
            Status::Verifying { start } => start,
        }
    }

    pub fn variant_str(&self) -> &'static str {
        match self {
            Status::WaitingForDownload { .. } => "waiting_for_download",
            Status::WaitingForCompute { .. } => "waiting_for_compute",
            Status::WaitingForUpload { .. } => "waiting_for_upload",
            Status::Verifying { .. } => "verifying",
        }
    }
}

pub struct ContributorState {
    pub updated_timestamp: DateTime<Utc>,
    pub contributor: Contributor,
    pub status: ContributorStatus,
}

pub enum ContributorStatus {
    DidntJoinQueue,
    Queued {
        joined: DateTime<Utc>,
        // Position shouldn't exist in the DB, but should be derived from the list of queued
        // contributors sorted by join time, where pos 0 means joined first.
        pos: usize,
    },
    Kicked {
        when: DateTime<Utc>,
        err: anyhow::Error,
    },
    Finished {
        // artifact
    },
}

// --- Helper types ---

#[derive(sqlx::FromRow)]
struct ContributorRow {
    verifying_key_hex: String,
    name: String,
    email: String,
    updated_timestamp: String,
    status: String,
    queued_joined_at: Option<String>,
    kicked_at: Option<String>,
    kicked_error: Option<String>,
    #[allow(dead_code)]
    finished_at: Option<String>,
}

#[derive(sqlx::FromRow)]
struct GlobalStatusRow {
    status: String,
    start: String,
}

// --- Helper functions ---

fn verifying_key_to_hex(key: &VerifyingKey) -> String {
    key.as_bytes()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}

fn hex_to_verifying_key(hex: &str) -> Result<VerifyingKey> {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
        .collect::<Result<Vec<u8>, _>>()?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid verifying key length"))?;
    Ok(VerifyingKey::from_bytes(&bytes)?)
}

impl From<&Contributor> for ContributorRow {
    fn from(c: &Contributor) -> Self {
        Self {
            verifying_key_hex: verifying_key_to_hex(&c.verifying_key),
            name: c.name.clone(),
            email: c.email.clone(),
            updated_timestamp: Utc::now().to_rfc3339(),
            status: "didnt_join_queue".to_string(),
            queued_joined_at: None,
            kicked_at: None,
            kicked_error: None,
            finished_at: None,
        }
    }
}

impl TryFrom<&ContributorRow> for Contributor {
    type Error = anyhow::Error;

    fn try_from(row: &ContributorRow) -> Result<Self> {
        let key = hex_to_verifying_key(&row.verifying_key_hex)?;
        Ok(Contributor {
            name: row.name.clone(),
            email: row.email.clone(),
            verifying_key: key,
        })
    }
}

impl TryFrom<(&ContributorRow, Option<usize>)> for ContributorStatus {
    type Error = anyhow::Error;

    fn try_from((row, pos): (&ContributorRow, Option<usize>)) -> Result<Self> {
        match row.status.as_str() {
            "didnt_join_queue" => Ok(ContributorStatus::DidntJoinQueue),
            "queued" => {
                let joined_str = row
                    .queued_joined_at
                    .as_ref()
                    .context("Missing queued_joined_at for queued contributor")?;
                let joined = DateTime::parse_from_rfc3339(joined_str)?.with_timezone(&Utc);
                Ok(ContributorStatus::Queued {
                    joined,
                    pos: pos.context("Missing position for queued contributor")?,
                })
            }
            "kicked" => {
                let when_str = row
                    .kicked_at
                    .as_ref()
                    .context("Missing kicked_at for kicked contributor")?;
                let when = DateTime::parse_from_rfc3339(when_str)?.with_timezone(&Utc);
                let err_msg = row
                    .kicked_error
                    .as_ref()
                    .context("Missing kicked_error for kicked contributor")?;
                Ok(ContributorStatus::Kicked {
                    when,
                    err: anyhow::anyhow!("{}", err_msg),
                })
            }
            "finished" => Ok(ContributorStatus::Finished {}),
            other => bail!("Unknown contributor status: {}", other),
        }
    }
}

impl TryFrom<&GlobalStatusRow> for Status {
    type Error = anyhow::Error;

    fn try_from(row: &GlobalStatusRow) -> Result<Self> {
        let start = DateTime::parse_from_rfc3339(&row.start)?.with_timezone(&Utc);
        match row.status.as_str() {
            "waiting_for_download" => Ok(Status::WaitingForDownload { start }),
            "waiting_for_compute" => Ok(Status::WaitingForCompute { start }),
            "waiting_for_upload" => Ok(Status::WaitingForUpload { start }),
            "verifying" => Ok(Status::Verifying { start }),
            other => bail!("Unknown global status: {}", other),
        }
    }
}



pub struct ContributorsDB {
    pool: SqlitePool,
}

impl ContributorsDB {
    pub async fn new(db_url: &str) -> Result<Self> {
        let pool = SqlitePool::connect(db_url).await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS contributors (
                verifying_key_hex TEXT PRIMARY KEY,
                name              TEXT NOT NULL,
                email             TEXT NOT NULL,
                updated_timestamp TEXT NOT NULL,
                status            TEXT NOT NULL DEFAULT 'didnt_join_queue',
                queued_joined_at  TEXT,
                kicked_at         TEXT,
                kicked_error      TEXT,
                finished_at       TEXT
            )",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS global_status (
                id     INTEGER PRIMARY KEY CHECK (id = 1),
                status TEXT NOT NULL,
                start  TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await?;

        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT OR IGNORE INTO global_status (id, status, start) VALUES (1, 'waiting_for_download', ?)",
        )
        .bind(&now)
        .execute(&pool)
        .await?;

        Ok(Self { pool })
    }

    /// Adds a contributor to the DB. Initial status should be `DidntJoinQueue`, updated_timestamp
    /// should be now
    pub async fn register(&mut self, contributor: &Contributor) -> Result<()> {
        let row = ContributorRow::from(contributor);
        sqlx::query(
            "INSERT INTO contributors (verifying_key_hex, name, email, updated_timestamp, status)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&row.verifying_key_hex)
        .bind(&row.name)
        .bind(&row.email)
        .bind(&row.updated_timestamp)
        .bind(&row.status)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Sets contributor status to Queued with joined: Utc::now().
    pub async fn enqueue(&mut self, contributor: &Contributor) -> Result<()> {
        let row = ContributorRow::from(contributor);
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE contributors
             SET status = 'queued', queued_joined_at = ?, updated_timestamp = ?,
                 kicked_at = NULL, kicked_error = NULL
             WHERE verifying_key_hex = ? AND status IN ('didnt_join_queue', 'kicked')",
        )
        .bind(&now)
        .bind(&now)
        .bind(&row.verifying_key_hex)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            bail!("Contributor not found or not in a state that can be enqueued");
        }
        Ok(())
    }

    /// Return a vec of all the contributors in the DB.
    pub async fn get_contributors(&self) -> Result<Vec<ContributorState>> {
        let rows: Vec<ContributorRow> =
            sqlx::query_as("SELECT * FROM contributors")
                .fetch_all(&self.pool)
                .await?;

        // Compute positions for queued contributors by sorting on queued_joined_at
        let mut queued_keys: Vec<(&str, &str)> = rows
            .iter()
            .filter(|r| r.status == "queued")
            .map(|r| {
                (
                    r.verifying_key_hex.as_str(),
                    r.queued_joined_at.as_deref().unwrap_or(""),
                )
            })
            .collect();
        queued_keys.sort_by_key(|&(_, t)| t);

        let positions: std::collections::HashMap<&str, usize> = queued_keys
            .iter()
            .enumerate()
            .map(|(i, (key, _))| (*key, i))
            .collect();

        let mut result = Vec::with_capacity(rows.len());
        for row in &rows {
            let contributor = Contributor::try_from(row)?;
            let pos = positions.get(row.verifying_key_hex.as_str()).copied();
            let status = ContributorStatus::try_from((row, pos))?;
            let updated_timestamp =
                DateTime::parse_from_rfc3339(&row.updated_timestamp)?.with_timezone(&Utc);
            result.push(ContributorState {
                updated_timestamp,
                contributor,
                status,
            });
        }

        Ok(result)
    }

    /// Set global status.
    pub async fn set_global_status(&mut self, status: Status) -> Result<()> {
        let variant = status.variant_str();
        let start = &status.start().to_rfc3339();
        sqlx::query("UPDATE global_status SET status = ?, start = ? WHERE id = 1")
            .bind(variant)
            .bind(&start)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Get global status.
    pub async fn get_global_status(&mut self) -> Result<Status> {
        let row: GlobalStatusRow =
            sqlx::query_as("SELECT status, start FROM global_status WHERE id = 1")
                .fetch_one(&self.pool)
                .await
                .context("Global status row not found")?;
        Status::try_from(&row)
    }

    /// Get contributor status
    pub async fn get_contributor_status(
        &mut self,
        contributor: &Contributor,
    ) -> Result<ContributorStatus> {
        let key_hex = ContributorRow::from(contributor).verifying_key_hex;
        let row: ContributorRow =
            sqlx::query_as("SELECT * FROM contributors WHERE verifying_key_hex = ?")
                .bind(&key_hex)
                .fetch_one(&self.pool)
                .await
                .context("Contributor not found")?;

        let pos = if row.status == "queued" {
            let joined_at = row
                .queued_joined_at
                .as_ref()
                .context("Missing queued_joined_at")?;
            let count: i32 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM contributors WHERE status = 'queued' AND queued_joined_at < ?",
            )
            .bind(joined_at)
            .fetch_one(&self.pool)
            .await?;
            Some(count as usize)
        } else {
            None
        };

        ContributorStatus::try_from((&row, pos))
    }

    pub async fn get_most_recent_finished_contributor(&mut self) -> Result<Contributor> {
        let row: ContributorRow = sqlx::query_as(
            "SELECT * FROM contributors WHERE status = 'finished' ORDER BY finished_at DESC LIMIT 1",
        )
        .fetch_one(&self.pool)
        .await
        .context("No finished contributors")?;
        Contributor::try_from(&row)
    }

    /// The "current" contributor is defined as the first queued contributor, sorted by joined
    /// timestamp.
    pub async fn get_current(&mut self) -> Result<ContributorState> {
        let row: ContributorRow = sqlx::query_as(
            "SELECT * FROM contributors WHERE status = 'queued' ORDER BY queued_joined_at ASC LIMIT 1",
        )
        .fetch_one(&self.pool)
        .await
        .context("No queued contributors")?;

        let contributor = Contributor::try_from(&row)?;
        let joined_str = row
            .queued_joined_at
            .as_ref()
            .context("Missing queued_joined_at")?;
        let joined = DateTime::parse_from_rfc3339(joined_str)?.with_timezone(&Utc);
        let updated_timestamp =
            DateTime::parse_from_rfc3339(&row.updated_timestamp)?.with_timezone(&Utc);

        Ok(ContributorState {
            updated_timestamp,
            contributor,
            status: ContributorStatus::Queued { joined, pos: 0 },
        })
    }

    /// Update the `updated_timestamp` field of the specified contributor in the DB.
    pub async fn update_timestamp(&mut self, contributor: &Contributor) -> Result<Contributor> {
        let key_hex = ContributorRow::from(contributor).verifying_key_hex;
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE contributors SET updated_timestamp = ? WHERE verifying_key_hex = ?")
            .bind(&now)
            .bind(&key_hex)
            .execute(&self.pool)
            .await?;
        Ok(contributor.clone())
    }

    /// Set the current contributor to be finished.
    pub async fn finish_current(&mut self) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE contributors SET status = 'finished', finished_at = ?, updated_timestamp = ?
             WHERE verifying_key_hex = (
                 SELECT verifying_key_hex FROM contributors
                 WHERE status = 'queued' ORDER BY queued_joined_at ASC LIMIT 1
             )",
        )
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            bail!("No queued contributor to finish");
        }

        self.set_global_status(Status::WaitingForDownload {
            start: Utc::now(),
        })
        .await?;
        Ok(())
    }

    /// Set the current contributor to be "kicked", and set global status to WaitingForDownload
    pub async fn kick_current(&mut self, e: anyhow::Error) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let err_msg = format!("{:#}", e);
        let result = sqlx::query(
            "UPDATE contributors SET status = 'kicked', kicked_at = ?, kicked_error = ?, updated_timestamp = ?
             WHERE verifying_key_hex = (
                 SELECT verifying_key_hex FROM contributors
                 WHERE status = 'queued' ORDER BY queued_joined_at ASC LIMIT 1
             )",
        )
        .bind(&now)
        .bind(&err_msg)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            bail!("No queued contributor to kick");
        }

        self.set_global_status(Status::WaitingForDownload {
            start: Utc::now(),
        })
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::thread_rng;

    #[tokio::test]
    async fn test_full_lifecycle() {
        let mut db = ContributorsDB::new("sqlite::memory:").await.unwrap();

        let mut rng = thread_rng();
        let (_, c1) = Contributor::new("Alice", "alice@example.com", &mut rng);
        let (_, c2) = Contributor::new("Bob", "bob@example.com", &mut rng);

        // Register first contributor -> DidntJoinQueue
        db.register(&c1).await.unwrap();
        let status = db.get_contributor_status(&c1).await.unwrap();
        assert!(matches!(status, ContributorStatus::DidntJoinQueue));

        // Enqueue first contributor -> Queued { pos: 0 }
        db.enqueue(&c1).await.unwrap();
        let status = db.get_contributor_status(&c1).await.unwrap();
        assert!(matches!(status, ContributorStatus::Queued { pos: 0, .. }));

        // Register + enqueue second contributor -> pos=1
        db.register(&c2).await.unwrap();
        db.enqueue(&c2).await.unwrap();
        let status = db.get_contributor_status(&c2).await.unwrap();
        assert!(matches!(status, ContributorStatus::Queued { pos: 1, .. }));

        // get_current returns first contributor
        let current = db.get_current().await.unwrap();
        assert_eq!(current.contributor, c1);

        // update_timestamp succeeds
        db.update_timestamp(&c1).await.unwrap();

        // finish_current -> first contributor is Finished
        db.finish_current().await.unwrap();
        let status = db.get_contributor_status(&c1).await.unwrap();
        assert!(matches!(status, ContributorStatus::Finished {}));

        // Second contributor becomes current
        let current = db.get_current().await.unwrap();
        assert_eq!(current.contributor, c2);

        // kick_current on second -> Kicked
        db.kick_current(anyhow::anyhow!("test error")).await.unwrap();
        let status = db.get_contributor_status(&c2).await.unwrap();
        assert!(matches!(status, ContributorStatus::Kicked { .. }));

        // Global status resets to WaitingForDownload
        let global = db.get_global_status().await.unwrap();
        assert!(matches!(global, Status::WaitingForDownload { .. }));

        // get_most_recent_finished_contributor returns first contributor
        let finished = db.get_most_recent_finished_contributor().await.unwrap();
        assert_eq!(finished, c1);
    }
}
