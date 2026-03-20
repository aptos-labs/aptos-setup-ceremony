use std::{collections::BTreeSet, path::Path};

use chrono::{DateTime, Utc};
use common::contribution::Contributor;

use crate::{messages::Msg, storage::{ContributorQueue, InactiveContributors}};



impl InactiveContributorState {
    pub fn starting_state(contributor: Contributor) -> Self {
        Self { 
            updated_timestamp: Utc::now(),
            contributor,
            status: InactiveContributorStatus::KickedOrDidntJoin,
        }
    }

    pub fn make_queued(self) -> QueuedContributorState {
        QueuedContributorState { 
            joined: Utc::now(),
            updated_timestamp: Utc::now(),
            contributor:  self.contributor,
        }
    }
}


impl State {
    pub fn initial(initial_authorized_contributors: Vec<Contributor>) -> Self {
        Self { 
            contributor_queue: ContributorQueue::new(),
            inactive_contributors: InactiveContributors::with_initialial_authorized_contributors(&initial_authorized_contributors),
            tick_timestamp: Utc::now(),
            current_status: Status::WaitingForDownload 
        }
    }

    pub fn register(self, contributor: &Contributor) -> Self {
        Self {

        }
    }

    pub fn step(self, msg: Msg) -> (Self, Effect) {
        match (self.current_status, msg) {
            (Status::WaitingForDownload, Msg::Tick(date_time)) => todo!(),
            (Status::WaitingForDownload, Msg::Register { contributor }) => todo!(),
            (Status::WaitingForDownload, Msg::Enqueue { contributor }) => todo!(),
            (Status::WaitingForDownload, Msg::RequestPosition { contributor }) => todo!(),
            (Status::WaitingForDownload, Msg::NotifyDownloadProgress { progress_percent }) => todo!(),
            (Status::WaitingForDownload, Msg::NotifyComputeProgress { progress_percent }) => todo!(),
            (Status::WaitingForDownload, Msg::NotifyUploadProgress { progress_percent }) => todo!(),
            (Status::WaitingForDownload, Msg::UploadFailed) => todo!(),
            (Status::WaitingForDownload, Msg::VerificationSucceeded) => todo!(),
            (Status::WaitingForDownload, Msg::VerificationFailed) => todo!(),
            (Status::WaitingForCompute, Msg::Tick(date_time)) => todo!(),
            (Status::WaitingForCompute, Msg::Register { contributor }) => todo!(),
            (Status::WaitingForCompute, Msg::Enqueue { contributor }) => todo!(),
            (Status::WaitingForCompute, Msg::RequestPosition { contributor }) => todo!(),
            (Status::WaitingForCompute, Msg::NotifyDownloadProgress { progress_percent }) => todo!(),
            (Status::WaitingForCompute, Msg::NotifyComputeProgress { progress_percent }) => todo!(),
            (Status::WaitingForCompute, Msg::NotifyUploadProgress { progress_percent }) => todo!(),
            (Status::WaitingForCompute, Msg::UploadFailed) => todo!(),
            (Status::WaitingForCompute, Msg::VerificationSucceeded) => todo!(),
            (Status::WaitingForCompute, Msg::VerificationFailed) => todo!(),
            (Status::WaitingForUpload, Msg::Tick(date_time)) => todo!(),
            (Status::WaitingForUpload, Msg::Register { contributor }) => todo!(),
            (Status::WaitingForUpload, Msg::Enqueue { contributor }) => todo!(),
            (Status::WaitingForUpload, Msg::RequestPosition { contributor }) => todo!(),
            (Status::WaitingForUpload, Msg::NotifyDownloadProgress { progress_percent }) => todo!(),
            (Status::WaitingForUpload, Msg::NotifyComputeProgress { progress_percent }) => todo!(),
            (Status::WaitingForUpload, Msg::NotifyUploadProgress { progress_percent }) => todo!(),
            (Status::WaitingForUpload, Msg::UploadFailed) => todo!(),
            (Status::WaitingForUpload, Msg::VerificationSucceeded) => todo!(),
            (Status::WaitingForUpload, Msg::VerificationFailed) => todo!(),
            (Status::Verifying, Msg::Tick(date_time)) => todo!(),
            (Status::Verifying, Msg::Register { contributor }) => todo!(),
            (Status::Verifying, Msg::Enqueue { contributor }) => todo!(),
            (Status::Verifying, Msg::RequestPosition { contributor }) => todo!(),
            (Status::Verifying, Msg::NotifyDownloadProgress { progress_percent }) => todo!(),
            (Status::Verifying, Msg::NotifyComputeProgress { progress_percent }) => todo!(),
            (Status::Verifying, Msg::NotifyUploadProgress { progress_percent }) => todo!(),
            (Status::Verifying, Msg::UploadFailed) => todo!(),
            (Status::Verifying, Msg::VerificationSucceeded) => todo!(),
            (Status::Verifying, Msg::VerificationFailed) => todo!(),
            (Status::Stopped, Msg::Tick(date_time)) => todo!(),
            (Status::Stopped, Msg::Register { contributor }) => todo!(),
            (Status::Stopped, Msg::Enqueue { contributor }) => todo!(),
            (Status::Stopped, Msg::RequestPosition { contributor }) => todo!(),
            (Status::Stopped, Msg::NotifyDownloadProgress { progress_percent }) => todo!(),
            (Status::Stopped, Msg::NotifyComputeProgress { progress_percent }) => todo!(),
            (Status::Stopped, Msg::NotifyUploadProgress { progress_percent }) => todo!(),
            (Status::Stopped, Msg::UploadFailed) => todo!(),
            (Status::Stopped, Msg::VerificationSucceeded) => todo!(),
            (Status::Stopped, Msg::VerificationFailed) => todo!(),
        }
        todo!()
    }

}
