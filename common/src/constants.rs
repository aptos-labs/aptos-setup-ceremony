use crate::{contribution::Contributor, fptx::FPTXParams};
use ed25519_dalek::SigningKey;
use lazy_static::lazy_static;
use rand::{SeedableRng as _, rngs::StdRng, thread_rng};


// real num_rounds is 216000
pub const PARAMS : FPTXParams = FPTXParams {
    batch_size: 128,
    num_rounds: 4000,
};


pub const UPLOAD_CHUNK_SIZE: usize = 64 * 1024 * 1024; // 8 MiB






// constants related to download/compute/upload test that client does
//

fn test_contrib_and_keypair() -> (SigningKey, Contributor) {
    Contributor::new(
        "Test",
        "test@test.com", 
        &mut StdRng::seed_from_u64(0)
    )
}

lazy_static! {
    pub static ref TEST_SIGNING_KEY : SigningKey  = test_contrib_and_keypair().0;
    pub static ref TEST_CONTRIBUTOR : Contributor = test_contrib_and_keypair().1;
}

pub const TEST_PARAMS : FPTXParams = FPTXParams {
    batch_size: 128,
    num_rounds: 4000,
};

// contributor w/ random VK so that test uploads don't conflict
pub fn test_upload_contributor() -> Contributor {
    Contributor::new("Test upload", "test upload", &mut thread_rng()).1
}

