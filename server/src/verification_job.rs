use common::contribution::Contributor;
use rayon;

pub struct VerificationJob {
    
}

impl VerificationJob {
    pub fn start(contributor: &Contributor) -> Self {
        rayon::spawn(|| {});
        todo!()
    }

    pub async fn finished(&self ) -> Result<()> {
        rayon::spawn(|| {});
        todo!()
    }
}
