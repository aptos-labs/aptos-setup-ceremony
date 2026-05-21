use std::time::Duration;

use crate::{aptos::{AptosContributionInner, AptosParams}, contribution::{Contribution, Contributor}};
use rand::thread_rng;
use lazy_static::lazy_static;

pub type CeremonyContribution = Contribution<AptosContributionInner>;

// real num_rounds is 216000
lazy_static! {
    pub static ref PARAMS : AptosParams = AptosParams::new(
        128, 
        216000, 
        256
    ).unwrap();
}

pub const PARALLEL_UPLOAD_PARTS: usize = 8;


// constants related to download/compute/upload test that client does

lazy_static! {
    pub static ref TEST_PARAMS : AptosParams = AptosParams::new(
        128, 
        2160, 
        8
    ).unwrap();
}

pub const TEST_DOWNLOAD_BLOB_SIZE_BYTES : usize = 1024 * 1024 * 512; // 512MB

// contributor w/ random VK so that test uploads don't conflict
pub fn test_upload_contributor() -> Contributor {
    Contributor::new("Test upload", "test upload", &mut thread_rng()).1
}


lazy_static! {
    // download timeout 90 => this should be 90/3.2
    pub static ref DOWNLOAD_TEST_CUTOFF : Duration = Duration::from_secs(10000);
    // contribute timeout 900 => this should be 900/100
    pub static ref COMPUTE_TEST_CUTOFF : Duration = Duration::from_secs(10000);
    // upload timeout 350 => this should be 450/3.2 (add padding)
    pub static ref UPLOAD_TEST_CUTOFF : Duration = Duration::from_secs(10000);
}
