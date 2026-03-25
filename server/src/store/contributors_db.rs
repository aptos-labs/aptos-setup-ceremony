use chrono::{DateTime, Utc};
use common::contribution::Contributor;
use ed25519_dalek::VerifyingKey;

use anyhow::{Context, Result, bail};
use sqlx::sqlite::{SqliteArguments, SqliteRow};
use sqlx::{Arguments, FromRow, Row, Sqlite, SqlitePool};

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

impl<'r> FromRow<'r, SqliteRow> for Status {
    fn from_row(row: &'r SqliteRow) -> Result<Self, sqlx::Error> {
        let status: String = row.try_get("status")?;
        let start_str: String = row.try_get("start")?;
        let start = DateTime::parse_from_rfc3339(&start_str)
            .map_err(|e| sqlx::Error::ColumnDecode {
                index: "start".to_string(),
                source: Box::new(e),
            })?
            .with_timezone(&Utc);
        match status.as_str() {
            "waiting_for_download" => Ok(Status::WaitingForDownload { start }),
            "waiting_for_compute" => Ok(Status::WaitingForCompute { start }),
            "waiting_for_upload" => Ok(Status::WaitingForUpload { start }),
            "verifying" => Ok(Status::Verifying { start }),
            other => Err(sqlx::Error::ColumnDecode {
                index: "status".to_string(),
                source: format!("Unknown status: {other}").into(),
            }),
        }
    }
}

impl<'q> sqlx::IntoArguments<'q, Sqlite> for &Status {
    fn into_arguments(self) -> SqliteArguments<'q> {
        let mut args = SqliteArguments::default();
        let _ = args.add(self.variant_str().to_string());
        let _ = args.add(self.start().to_rfc3339());
        args
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

impl<'r> FromRow<'r, SqliteRow> for ContributorState {
    fn from_row(row: &'r SqliteRow) -> Result<Self, sqlx::Error> {
        let hex: String = row.try_get("verifying_key_hex")?;
        let key = VerifyingKey::try_from(&Hex::from(hex.as_str())).map_err(|e| {
            sqlx::Error::ColumnDecode {
                index: "verifying_key_hex".to_string(),
                source: format!("{e}").into(),
            }
        })?;
        let contributor = Contributor {
            name: row.try_get("name")?,
            email: row.try_get("email")?,
            verifying_key: key,
        };

        let updated_ts: String = row.try_get("updated_timestamp")?;
        let updated_timestamp = DateTime::parse_from_rfc3339(&updated_ts)
            .map_err(|e| sqlx::Error::ColumnDecode {
                index: "updated_timestamp".to_string(),
                source: Box::new(e),
            })?
            .with_timezone(&Utc);

        let status_str: String = row.try_get("status")?;
        let status = match status_str.as_str() {
            "didnt_join_queue" => ContributorStatus::DidntJoinQueue,
            "queued" => {
                let joined_str: Option<String> = row.try_get("queued_joined_at")?;
                let joined_str = joined_str.ok_or_else(|| sqlx::Error::ColumnDecode {
                    index: "queued_joined_at".to_string(),
                    source: "Missing queued_joined_at for queued contributor".into(),
                })?;
                let joined = DateTime::parse_from_rfc3339(&joined_str)
                    .map_err(|e| sqlx::Error::ColumnDecode {
                        index: "queued_joined_at".to_string(),
                        source: Box::new(e),
                    })?
                    .with_timezone(&Utc);
                let pos: i32 = row.try_get("pos")?;
                ContributorStatus::Queued {
                    joined,
                    pos: pos as usize,
                }
            }
            "kicked" => {
                let when_str: Option<String> = row.try_get("kicked_at")?;
                let when_str = when_str.ok_or_else(|| sqlx::Error::ColumnDecode {
                    index: "kicked_at".to_string(),
                    source: "Missing kicked_at for kicked contributor".into(),
                })?;
                let when = DateTime::parse_from_rfc3339(&when_str)
                    .map_err(|e| sqlx::Error::ColumnDecode {
                        index: "kicked_at".to_string(),
                        source: Box::new(e),
                    })?
                    .with_timezone(&Utc);
                let err_msg: Option<String> = row.try_get("kicked_error")?;
                let err_msg = err_msg.ok_or_else(|| sqlx::Error::ColumnDecode {
                    index: "kicked_error".to_string(),
                    source: "Missing kicked_error for kicked contributor".into(),
                })?;
                ContributorStatus::Kicked {
                    when,
                    err: anyhow::anyhow!("{}", err_msg),
                }
            }
            "finished" => ContributorStatus::Finished {},
            other => {
                return Err(sqlx::Error::ColumnDecode {
                    index: "status".to_string(),
                    source: format!("Unknown contributor status: {other}").into(),
                })
            }
        };

        Ok(ContributorState {
            updated_timestamp,
            contributor,
            status,
        })
    }
}

// --- Hex newtype for VerifyingKey serialization ---

struct Hex(String);

impl From<&VerifyingKey> for Hex {
    fn from(key: &VerifyingKey) -> Self {
        Hex(key
            .as_bytes()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect())
    }
}

impl TryFrom<&Hex> for VerifyingKey {
    type Error = anyhow::Error;

    fn try_from(hex: &Hex) -> Result<Self> {
        let bytes: Vec<u8> = (0..hex.0.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex.0[i..i + 2], 16))
            .collect::<Result<Vec<u8>, _>>()?;
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid verifying key length"))?;
        Ok(VerifyingKey::from_bytes(&bytes)?)
    }
}

impl From<Hex> for String {
    fn from(hex: Hex) -> Self {
        hex.0
    }
}

impl From<&str> for Hex {
    fn from(s: &str) -> Self {
        Hex(s.to_string())
    }
}

fn contributor_key_hex(contributor: &Contributor) -> String {
    String::from(Hex::from(&contributor.verifying_key))
}

/// Builds a SELECT query that includes a computed `pos` column for queued contributors.
fn select_with_pos(suffix: &str) -> String {
    format!(
        "SELECT c1.*, \
         CASE WHEN c1.status = 'queued' THEN \
           (SELECT COUNT(*) FROM contributors c2 \
            WHERE c2.status = 'queued' AND c2.queued_joined_at < c1.queued_joined_at) \
         ELSE 0 END as pos \
         FROM contributors c1 {suffix}"
    )
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
        let mut args = SqliteArguments::default();
        let _ = args.add(contributor_key_hex(contributor));
        let _ = args.add(contributor.name.clone());
        let _ = args.add(contributor.email.clone());
        let _ = args.add(Utc::now().to_rfc3339());
        let _ = args.add("didnt_join_queue".to_string());
        sqlx::query_with(
            "INSERT INTO contributors (verifying_key_hex, name, email, updated_timestamp, status)
             VALUES (?, ?, ?, ?, ?)",
            args,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Sets contributor status to Queued with joined: Utc::now().
    pub async fn enqueue(&mut self, contributor: &Contributor) -> Result<()> {
        let key_hex = contributor_key_hex(contributor);
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE contributors
             SET status = 'queued', queued_joined_at = ?, updated_timestamp = ?,
                 kicked_at = NULL, kicked_error = NULL
             WHERE verifying_key_hex = ? AND status IN ('didnt_join_queue', 'kicked')",
        )
        .bind(&now)
        .bind(&now)
        .bind(&key_hex)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            bail!("Contributor not found or not in a state that can be enqueued");
        }
        Ok(())
    }

    /// Return a vec of all the contributors in the DB.
    pub async fn get_contributors(&self) -> Result<Vec<ContributorState>> {
        let states: Vec<ContributorState> = sqlx::query_as(&select_with_pos(""))
            .fetch_all(&self.pool)
            .await?;
        Ok(states)
    }

    /// Set global status.
    pub async fn set_global_status(&mut self, status: Status) -> Result<()> {
        sqlx::query_with(
            "UPDATE global_status SET status = ?, start = ? WHERE id = 1",
            &status,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Get global status.
    pub async fn get_global_status(&mut self) -> Result<Status> {
        let status: Status =
            sqlx::query_as("SELECT status, start FROM global_status WHERE id = 1")
                .fetch_one(&self.pool)
                .await
                .context("Global status row not found")?;
        Ok(status)
    }

    /// Get contributor status
    pub async fn get_contributor_status(
        &mut self,
        contributor: &Contributor,
    ) -> Result<ContributorStatus> {
        let key_hex = contributor_key_hex(contributor);
        let state: ContributorState =
            sqlx::query_as(&select_with_pos("WHERE c1.verifying_key_hex = ?"))
                .bind(&key_hex)
                .fetch_one(&self.pool)
                .await
                .context("Contributor not found")?;
        Ok(state.status)
    }

    pub async fn get_most_recent_finished_contributor(&mut self) -> Result<Contributor> {
        let state: ContributorState = sqlx::query_as(&select_with_pos(
            "WHERE c1.status = 'finished' ORDER BY c1.finished_at DESC LIMIT 1",
        ))
        .fetch_one(&self.pool)
        .await
        .context("No finished contributors")?;
        Ok(state.contributor)
    }

    /// The "current" contributor is defined as the first queued contributor, sorted by joined
    /// timestamp.
    pub async fn get_current(&mut self) -> Result<ContributorState> {
        let state: ContributorState = sqlx::query_as(&select_with_pos(
            "WHERE c1.status = 'queued' ORDER BY c1.queued_joined_at ASC LIMIT 1",
        ))
        .fetch_one(&self.pool)
        .await
        .context("No queued contributors")?;
        Ok(state)
    }

    /// Update the `updated_timestamp` field of the specified contributor in the DB.
    pub async fn update_timestamp(&mut self, contributor: &Contributor) -> Result<Contributor> {
        let key_hex = contributor_key_hex(contributor);
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
        db.kick_current(anyhow::anyhow!("test error"))
            .await
            .unwrap();
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
