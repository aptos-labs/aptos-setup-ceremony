use common::contribution::Contributor;

use crate::state::{InactiveContributorState, QueuedContributorState};

pub struct ContributorQueue {}

// TODO need to support "atomic" updates. I.e., remove from inactive -> add to queue, or remove
// from queue -> add to inactive
//
// Proposed strategy: 
// - sqlite db of (message, timestamp) pairs
// - "update" : State -> State
//   - Only updates the struct, doesn't e.g. do verification or initialize GCS buckets (maybe
//   checks whether they exist? might be slow though... but maybe this is fine)
// - Mutex, _and_ "update_in_progress" flag in sqlite
// - "initialize_with_db" fn that reads all updates, checks update_in_progress, fast-forwards to
// current state, if download/upload/verification in progress, try to reinitialize storage bucket
// or job
// inspiration:
// https://hoverbear.org/blog/rust-state-machine-pattern/
// actually, this is cleaner:
// https://play.rust-lang.org/?version=stable&mode=debug&edition=2015&gist=ee3e4df093c136ced7b394dc7ffb78e1

impl ContributorQueue {
    pub fn update(&mut self, contributor: &Contributor, updated_state: QueuedContributorState) {
    }

    pub fn first(&self) -> QueuedContributorState {
        todo!()
    }
    pub fn dequeue_first(&mut self) {
    }

    pub fn new() -> Self { todo!() }
}


pub struct InactiveContributors {
}

impl InactiveContributors {
    pub fn as_vec(&self) -> Vec<InactiveContributorState> {
        todo!()
    }

    pub fn with_initialial_authorized_contributors(initial: &Vec<Contributor>) -> Self {
        todo!()
    }

    pub fn remove(&mut self, contributor: &Contributor) {
    }

    pub fn finish(&mut self, contributor: &Contributor, artifact: ()) {

    }

    // kick or add for the first time
    pub fn add(&mut self, contributor: &Contributor) {

    }
}
