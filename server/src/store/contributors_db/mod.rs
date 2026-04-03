
use common::contribution::{AsAndFromHex, Contributor};

use anyhow::{Context, Result, bail};
use sqlx::SqlitePool;

use crate::store::contributors_db::types::{ContributorRow, ContributorRowWithPos};

pub mod types;



pub fn anyhow_to_string<S: serde::Serializer>(err: &anyhow::Error, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&format!("{}", err))
}

pub fn str_to_anyhow<'de, D: serde::de::Deserializer<'de>>(data: D) -> Result<anyhow::Error, D::Error> {
    let s: String = serde::de::Deserialize::deserialize(data)?;
    Ok(anyhow::Error::msg(s))
}



/// Builds a SELECT query that includes a computed `pos` column for queued contributors.
fn select_with_pos(suffix: &str) -> String {
    format!(
        "SELECT c1.*, \
         CASE WHEN c1.status = 'queued' THEN \
           (SELECT COUNT(*) FROM contributors c2 \
            WHERE (c2.status = 'queued') AND c2.joined_at < c1.joined_at) \
         ELSE 0 END as pos \
         FROM contributors c1 {suffix}"
    )
}

pub struct ContributorsDB {
    pub(crate) pool: SqlitePool,
}

impl ContributorsDB {
    pub async fn new(db_url: &str) -> Result<Self> {
        let pool = SqlitePool::connect(db_url).await?;
        ContributorRow::init_table(&pool).await?;
        Ok(Self { pool })
    }

    /// Adds a contributor to the DB. Initial status should be `DidntJoinQueue`, updated_timestamp
    /// should be now
    pub async fn register(&mut self, contributor: &Contributor) -> Result<()> {
        ContributorRow::new(contributor.clone())
            .insert(&self.pool)
            .await
    }


    /// Return a vec of all the contributors in the DB.
    pub async fn get_contributors(&self) -> Result<Vec<(usize,ContributorRow)>> {
        let states: Vec<(usize, ContributorRow)> = sqlx::query_as(&select_with_pos(""))
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|r: ContributorRowWithPos| (r.pos(), r.into_row()))
            .collect();
        Ok(states)
    }

    /// Get contributor status
    pub async fn get_with_pos(
        &self,
        contributor: &Contributor,
    ) -> Result<(usize, ContributorRow)> {
        let row : ContributorRowWithPos =
            sqlx::query_as(&select_with_pos("WHERE c1.verifying_key = ?"))
                .bind(&contributor.verifying_key.as_hex()?)
                .fetch_one(&self.pool)
                .await
                .context("Contributor not found")?;

        Ok((row.pos(), row.into_row()))
    }

    pub async fn get_finished_contributors(&self) -> Result<Vec<ContributorRow>> {
        let states: Vec<ContributorRow> = sqlx::query_as(&select_with_pos(
            "WHERE c1.status = 'finished' ORDER BY c1.finished_verify_at ASC",
        ))
        .fetch_all(&self.pool)
        .await?;
        Ok(states)
    }

    pub async fn get_most_recent_finished_contributor(&mut self) -> Result<Option<Contributor>> {
        let state: Option<ContributorRow> = sqlx::query_as(&select_with_pos(
            "WHERE c1.status = 'finished' ORDER BY c1.finished_verify_at DESC LIMIT 1",
        ))
        .fetch_optional(&self.pool)
        .await?;
        Ok(state.map(|s| s.contributor()))
    }

    /// The "current" contributor is defined as the first queued contributor, sorted by joined
    /// timestamp.
    pub async fn get_current(&self) -> Result<Option<ContributorRow>> {
        let state: Option<ContributorRow> = sqlx::query_as(&select_with_pos(
            "WHERE c1.status = 'queued' ORDER BY c1.joined_at ASC LIMIT 1",
        ))
        .fetch_optional(&self.pool)
        .await?;
        Ok(state)
    }

    pub async fn finish_current(&self) -> Result<()> {
        match self.get_current().await? {
            Some(current) => {
                current.mark_finished_verify(&self.pool).await
            }
            None => bail!("No current contributor"),
        }
    }

    pub async fn kick_current(&self, error: &str) -> Result<()> {
        match self.get_current().await? {
            Some(current) => {
                current.mark_kicked(error, &self.pool).await
            }
            None => bail!("No current contributor"),
        }
    }
}

