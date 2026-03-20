use rayon;

pub struct VerificationJob {
}

impl VerificationJob {
    pub fn start() -> Self {
        rayon::spawn(|| {});
        todo!()
    }
}
