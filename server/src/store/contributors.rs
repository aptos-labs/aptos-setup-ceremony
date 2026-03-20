use chrono::{DateTime, Utc};
use common::contribution::Contributor;

use anyhow::Result;


pub enum Status {
    WaitingForDownload,
    WaitingForCompute,
    WaitingForUpload,
    Verifying,
    Stopped
}

#[derive(Eq, PartialEq)]
pub struct ContributorState {
    updated_timestamp: DateTime<Utc>,
    contributor: Contributor,
    status: ContributorStatus,
}

#[derive(Eq, PartialEq)]
pub enum ContributorStatus {
    DidntJoinQueue,
    Queued {
        joined: DateTime<Utc>,
        pos: usize,
    },
    Kicked,
    Finished {
        // artifact
    }
}


pub struct ContributorsDB {
}



impl ContributorsDB {
    /// Adds a contributor to the DB. Initial status should be `DidntJoinQueue`, updated_timestamp
    /// should be now
    pub async fn register(&mut self, contributor: &Contributor) -> Result<()> {
        todo!()
    }

    /// Return a vec of all the contributors in the DB.
    pub async fn get_contributors(&self) -> Result<Vec<ContributorState>> {
        todo!()
    }

    /// Set global status.
    pub async fn set_global_status(&mut self, status: Status) -> Result<()> {
        todo!()
    }

    /// Set global status.
    pub async fn get_global_status(&mut self) -> Result<Status> {
        todo!()
    }

    /// Get contributor status 
    pub async fn get_contributor_status(&mut self, contributor: &Contributor) -> Result<ContributorStatus> {
        todo!()
    }

    pub async fn get_most_recent_finished_contributor(&mut self) -> Result<Contributor> {
        todo!()
    }

    /// The "current" contributor is defined as the first queued contributor, sorted by joined
    /// timestamp.
    pub async fn get_current(&mut self) -> Result<Contributor> {
        todo!()
    }

    /// Update the `updated_timestamp` field of the specified contributor in the DB.
    pub async fn update_timestamp(&mut self, contributor: &Contributor) -> Result<Contributor> {
        todo!()
    }

    /// Set the current contributor to be finished. 
    pub async fn finish_current(&mut self) -> Result<()> {
        todo!()
    }

    /// Set the current contributor to be "kicked".
    pub async fn kick_current(&mut self) -> Result<()> {
        todo!()
    }

}
