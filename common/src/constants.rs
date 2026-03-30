use crate::{contribution::Contributor, fptx::FPTXParams};
use rand::thread_rng;


// real num_rounds is 216000
pub const PARAMS : FPTXParams = FPTXParams {
    batch_size: 128,
    num_rounds: 4000,
};


pub const UPLOAD_CHUNK_SIZE: usize = 64 * 1024 * 1024; // 8 MiB (is this right?)






// constants related to download/compute/upload test that client does
//


pub const TEST_PARAMS : FPTXParams = FPTXParams {
    batch_size: 128,
    num_rounds: 4000,
};

pub const TEST_DOWNLOAD_BLOB_SIZE_BYTES : usize = 1024 * 1024 * 128; // 128MB

// contributor w/ random VK so that test uploads don't conflict
pub fn test_upload_contributor() -> Contributor {
    Contributor::new("Test upload", "test upload", &mut thread_rng()).1
}

