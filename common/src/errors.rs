use thiserror::Error;



#[derive(Debug, Error)]
pub enum ContributionVerificationFailure {
    #[error("Contribution hashes mismatch")]
    ContributionHashMismatch
}

#[derive(Debug, Error)]
#[error("Error deserializing contribution: {0}")]
pub struct DeserializationError(pub bcs::Error);

#[derive(Debug, Error)]
#[error("FPTX batch size must be a power of two")]
pub struct BatchSizeNotPowerOfTwo;
