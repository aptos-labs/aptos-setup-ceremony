use std::time::Duration;

use crate::{contribution::Contributor, fptx::FPTXParams};
use rand::thread_rng;
use lazy_static::lazy_static;


// real num_rounds is 216000
pub const PARAMS : FPTXParams = FPTXParams {
    batch_size: 128,
    num_rounds: 2160,
};


pub const UPLOAD_CHUNK_SIZE: usize = 64 * 1024 * 1024; // 8 MiB (is this right?)






// constants related to download/compute/upload test that client does


pub const TEST_PARAMS : FPTXParams = FPTXParams {
    batch_size: 128,
    num_rounds: 1, // 1/100th of the real size
};

pub const TEST_DOWNLOAD_BLOB_SIZE_BYTES : usize = 1024 * 1024 * 128; // 128MB

// contributor w/ random VK so that test uploads don't conflict
pub fn test_upload_contributor() -> Contributor {
    Contributor::new("Test upload", "test upload", &mut thread_rng()).1
}


lazy_static! {
    pub static ref DOWNLOAD_TEST_CUTOFF : Duration = Duration::from_secs(20);
    // 12 seconds for 1/100th of the real size => ~1200 secs = 20 mins for the real size. Note
    //    that this doesn't test deserialization so there is built-in inaccuracy
    pub static ref COMPUTE_TEST_CUTOFF : Duration = Duration::from_secs(12);
    pub static ref UPLOAD_TEST_CUTOFF : Duration = Duration::from_secs(20);
}
