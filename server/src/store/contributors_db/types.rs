use chrono::{DateTime, Utc};
use common::contribution::{AsAndFromHex as _, Contributor};
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize, Serializer};
use sqlx::{Database, Decode, Encode,  encode::IsNull, prelude::FromRow};
use tabled::Tabled;
use core::error::Error;
use std::fmt::Display;
use sqlx::{SqlitePool, Type};


#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub enum CurrentContributionStep {
    DownloadNotStarted,
    DownloadStarted { start: DateTime<Utc> },
    ComputeStarted { start: DateTime<Utc> },
    UploadStarted { start: DateTime<Utc> },
    Verifying 
}

impl CurrentContributionStep {
    pub fn variant_name(&self) -> &'static str {
        match self {
            CurrentContributionStep::DownloadNotStarted => "download not started",
            CurrentContributionStep::DownloadStarted { .. } => "download started",
            CurrentContributionStep::ComputeStarted { .. } => "compute started",
            CurrentContributionStep::UploadStarted { .. } => "upload started",
            CurrentContributionStep::Verifying => "verification started",
        }
    }
}

#[derive(sqlx::Type, PartialEq, Eq, Tabled, Debug, Clone, Serialize, Deserialize, PartialOrd, Ord, Copy)]
#[sqlx(type_name = "contributor_status")]
#[sqlx(rename_all = "lowercase")]
pub enum ContributorStatus {
    DidntJoinQueue=2,
    Queued=3,
    Kicked=1,
    Finished=4,
}


fn hex_serialize<S>(x: &VerifyingKey, s: S) -> Result<S::Ok, S::Error>
where 
    S: Serializer
{
    s.serialize_str(&x.as_hex().unwrap())
}

// So that I can derive sqlx-related traits
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct VKWrapper(
    #[serde(serialize_with = "hex_serialize")]
    VerifyingKey
);

impl Display for VKWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.as_hex().map_err(|_| std::fmt::Error::default())?)
    }
}

impl AsRef<VerifyingKey> for VKWrapper {
    fn as_ref(&self) -> &VerifyingKey {
        &self.0
    }
}

impl From<VerifyingKey> for VKWrapper {
    fn from(value: VerifyingKey) -> Self {
        Self(value)
    }
}


impl<'q, DB: Database> Encode<'q, DB> for VKWrapper 
    where String: Encode<'q, DB>
{
    fn encode_by_ref(
        &self,
        buf: &mut <DB as sqlx::Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, Box<dyn Error + Send + Sync + 'static>> {
        Encode::<DB>::encode(self.0.as_hex()?, buf)
    }
}

impl<'r, DB: Database> Decode<'r, DB> for VKWrapper
where
    &'r str: Decode<'r, DB>
{
    fn decode(
        value: <DB as Database>::ValueRef<'r>,
    ) -> Result<Self, Box<dyn Error + 'static + Send + Sync>> {
        Ok(
            Self(
                VerifyingKey::from_hex(
                    <&str as Decode<DB>>::decode(value)?
                )?
            )
        )
    }
}

impl<DB: Database> Type<DB> for VKWrapper
    where String: Type<DB>
{
    fn type_info() -> <DB as Database>::TypeInfo {
        <String as Type<DB>>::type_info()
    }
}

#[derive(FromRow, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContributorRowWithPos {
    pub pos: u64,
    #[sqlx(flatten)]
    pub row: ContributorRow,
}

impl ContributorRowWithPos {
    pub fn into_row(self) -> ContributorRow {
        self.row
    }
    pub fn pos(&self) -> usize {
        self.pos as usize
    }
}

fn format_datetime(d: Option<DateTime<Utc>>) -> String {
    match d {
        Some(d) => format!("{:?}", d),
        None => format!("")
    }
}

#[derive(Tabled, FromRow, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContributorRow {
    pub updated_timestamp: DateTime<Utc>,
    #[tabled(format("{:?}", self.verifying_key.0.as_hex()))]
    pub verifying_key: VKWrapper,
    pub name: String,
    pub email: String,
    #[tabled(format("{:?}", self.status))]
    pub status: ContributorStatus,
    #[tabled(inline)]
    pub test_download_secs: Option<u32>,
    #[tabled(inline)]
    pub test_compute_secs: Option<u32>,
    #[tabled(inline)]
    pub test_upload_secs: Option<u32>,
    #[tabled(format("{:?}", format_datetime(self.joined_at)))]
    pub joined_at: Option<DateTime<Utc>>,
    #[tabled(format("{:?}", format_datetime(self.kicked_at)))]
    pub kicked_at: Option<DateTime<Utc>>,
    #[tabled(inline)]
    pub kicked_error: Option<String>,
    #[tabled(format("{:?}", format_datetime(self.started_download_at)))]
    pub started_download_at: Option<DateTime<Utc>>,
    #[tabled(format("{:?}", self.started_compute_at))]
    #[tabled(format("{:?}", format_datetime(self.started_compute_at)))]
    pub started_compute_at: Option<DateTime<Utc>>,
    #[tabled(format("{:?}", format_datetime(self.started_upload_at)))]
    pub started_upload_at: Option<DateTime<Utc>>,
    #[tabled(format("{:?}", self.finished_upload_at))]
    pub finished_upload_at: Option<DateTime<Utc>>,
    #[tabled(inline)]
    pub contribution_hash: Option<String>,
    #[tabled(format("{:?}", format_datetime(self.finished_verify_at)))]
    pub finished_verify_at: Option<DateTime<Utc>>,
}


impl ContributorRow {
    pub fn contributor(&self) -> Contributor {
        Contributor { name: self.name.clone(), email: self.email.clone(), verifying_key: *self.verifying_key.as_ref() }
    }

    pub fn with_pos(self, pos: u64) -> ContributorRowWithPos {
        ContributorRowWithPos { pos, row: self }
    }

    pub async fn init_table(pool: &SqlitePool) -> anyhow::Result<()> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS contributors (
                verifying_key        TEXT   PRIMARY  KEY,            
                name                 TEXT   NOT      NULL,           
                email                TEXT   NOT      NULL,           
                updated_timestamp    TEXT   NOT      NULL,           
                status               TEXT   NOT      NULL   DEFAULT  'didnt_join_queue',
                test_download_secs   INTEGER,                           
                test_compute_secs    INTEGER,                           
                test_upload_secs     INTEGER,                           
                joined_at            TEXT,                           
                kicked_at            TEXT,                           
                kicked_error         TEXT,                           
                started_download_at  TEXT,                           
                started_compute_at   TEXT,                           
                started_upload_at    TEXT,                           
                finished_upload_at   TEXT,                           
                contribution_hash    TEXT,
                finished_verify_at   TEXT
            )",
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    pub fn new(c: Contributor) -> Self {
        Self {
            updated_timestamp: Utc::now(),
            verifying_key: c.verifying_key.into(),
            name: c.name,
            email: c.email,
            status: ContributorStatus::DidntJoinQueue,
            test_download_secs: None,
            test_compute_secs: None,
            test_upload_secs: None,
            joined_at: None,
            kicked_at: None,
            kicked_error: None,
            started_download_at: None,
            started_compute_at: None,
            started_upload_at: None,
            finished_upload_at: None,
            contribution_hash: None,
            finished_verify_at: None,
        }
    }

    pub async fn insert(&self, pool: &SqlitePool) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO contributors VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        ).bind(&self.verifying_key)
            .bind(&self.name)
            .bind(&self.email)
            .bind(self.updated_timestamp)
            .bind(&self.status)
            .bind(&self.test_download_secs)
            .bind(&self.test_compute_secs)
            .bind(&self.test_upload_secs)
            .bind(self.joined_at)
            .bind(self.kicked_at)
            .bind(&self.kicked_error)
            .bind(self.started_download_at)
            .bind(self.started_compute_at)
            .bind(self.started_upload_at)
            .bind(self.finished_upload_at)
            .bind(&self.contribution_hash)
            .bind(&self.finished_verify_at)
            .execute(pool).await?;

        Ok(())
    }

    pub async fn enqueue(
        self, 
        pool: &SqlitePool,
        download_secs: u64,
        compute_secs: u64,
        upload_secs: u64,
    ) -> anyhow::Result<()> {
        let now = Utc::now();

        // clear out all active-contributor timestamps on join/rejoin
        sqlx::query(
            "UPDATE contributors SET status=?, joined_at=?, updated_timestamp=?,
            test_download_secs=?, test_compute_secs=?, test_upload_secs=?,
            kicked_at=NULL, kicked_error=NULL, started_download_at=NULL,
            started_compute_at=NULL, started_upload_at=NULL, 
            finished_upload_at=NULL, contribution_hash=NULL WHERE verifying_key = ?",
        )
            .bind(ContributorStatus::Queued)
            .bind(Some(now))
            .bind(now)
            .bind(download_secs as u32)
            .bind(compute_secs as u32)
            .bind(upload_secs as u32)
            .bind(self.verifying_key)
            .execute(pool).await?;

        Ok(())
    }

    pub async fn update_timestamp(self, pool: &SqlitePool) -> anyhow::Result<()> {
        let now = Utc::now();

        sqlx::query(
            "UPDATE contributors SET updated_timestamp=? WHERE verifying_key=?",
        )
            .bind(now)
            .bind(self.verifying_key)
            .execute(pool).await?;

        Ok(())
    }

    pub async fn mark_started_download(self, pool: &SqlitePool) -> anyhow::Result<()> {
        let now = Utc::now();

        sqlx::query(
            "UPDATE contributors SET started_download_at=?, updated_timestamp=? WHERE verifying_key=?",
        )
            .bind(Some(now))
            .bind(now)
            .bind(self.verifying_key)
            .execute(pool).await?;

        Ok(())
    }

    pub async fn mark_started_compute(self, pool: &SqlitePool) -> anyhow::Result<()> {
        let now = Utc::now();

        sqlx::query(
            "UPDATE contributors SET started_compute_at=?, updated_timestamp=? WHERE verifying_key=?",
        )
            .bind(now)
            .bind(now)
            .bind(self.verifying_key)
            .execute(pool).await?;

        Ok(())
    }

    pub async fn mark_started_upload(self, pool: &SqlitePool) -> anyhow::Result<()> {
        let now = Utc::now();

        sqlx::query(
            "UPDATE contributors SET started_upload_at=?, updated_timestamp=? WHERE verifying_key=?",
        )
            .bind(now)
            .bind(now)
            .bind(self.verifying_key)
            .execute(pool).await?;

        Ok(())
    }

    pub async fn mark_finished_upload(self, contribution_hash: String, pool: &SqlitePool) -> anyhow::Result<()> {
        let now = Utc::now();

        sqlx::query(
            "UPDATE contributors SET finished_upload_at=?, updated_timestamp=?, contribution_hash=? WHERE verifying_key=?",
        )
            .bind(now)
            .bind(Some(now))
            .bind(contribution_hash)
            .bind(self.verifying_key)
            .execute(pool).await?;

        Ok(())
    }

    pub async fn mark_finished_verify(self, pool: &SqlitePool) -> anyhow::Result<()> {
        let now = Utc::now();

        sqlx::query(
            "UPDATE contributors SET status=?, finished_verify_at=?, updated_timestamp=? WHERE verifying_key=?",
        )
            .bind(ContributorStatus::Finished)
            .bind(now)
            .bind(Some(now))
            .bind(self.verifying_key)
            .execute(pool).await?;

        Ok(())
    }

    pub async fn mark_kicked(&self, kicked_error: &str, pool: &SqlitePool) -> anyhow::Result<()> {
        let now = Utc::now();

        sqlx::query(
            "UPDATE contributors SET status=?, kicked_at = ?, updated_timestamp=?, kicked_error = ? WHERE verifying_key = ?",
        )
            .bind(ContributorStatus::Kicked)
            .bind(Some(now))
            .bind(now)
            .bind(kicked_error)
            .bind(&self.verifying_key)
            .execute(pool).await?;

        Ok(())
    }

    pub fn get_current_contribution_step(&self) -> CurrentContributionStep {
        if let Some(_) = self.finished_upload_at {
            CurrentContributionStep::Verifying  
        } else if let Some(start) = self.started_upload_at {
            CurrentContributionStep::UploadStarted {start}
        } else if let Some(start) = self.started_compute_at {
            CurrentContributionStep::ComputeStarted {start}
        } else if let Some(start) = self.started_download_at {
            CurrentContributionStep::DownloadStarted {start: start }
        } else {
            CurrentContributionStep::DownloadNotStarted
        }
    }
}

#[cfg(test)]
mod tests {
    use common::contribution::Contributor;
    use rand::thread_rng;
    use sqlx::SqlitePool;

    use crate::store::contributors_db::types::ContributorRow;


    #[tokio::test]
    async fn test_contributor_row() {
        let mut pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

        ContributorRow::init_table(&mut pool).await.unwrap();

        let mut rng = thread_rng();
        let (_, c) = Contributor::new("Alice", "alice@example.com", &mut rng);

        let row = ContributorRow::new(c);
        row.insert(&pool).await.unwrap();

        let row_fetched: ContributorRow = sqlx::query_as("SELECT * FROM contributors")
        .fetch_one(&pool).await.unwrap();
        
        assert_eq!(row, row_fetched);
    }
}
