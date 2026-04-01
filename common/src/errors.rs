use thiserror::Error;


#[derive(Debug, Error, Clone)]
pub enum ContributionGenerationFailure {
    #[error("Params mismatch")]
    ParamsMismatch,
}


#[derive(Debug, Error, Clone)]
pub enum ContributionVerificationFailure {
    #[error("Contribution hashes mismatch")]
    ContributionHashMismatch,
    #[error("Multipairing equation had a nonzero result")]
    MultipairingEquationNonZeroResult,
    #[error("Params mismatch")]
    ParamsMismatch,
}

#[derive(Debug, Error)]
#[error("Error deserializing contribution: {0}")]
pub struct DeserializationError(pub bcs::Error);

#[derive(Debug, Error)]
#[error("FPTX batch size must be a power of two")]
pub struct BatchSizeNotPowerOfTwo;

#[derive(Debug, Error)]
#[error("Signature of knowledge verification failed")]
pub struct SoKVerificationError;

#[derive(Debug, Error)]
#[error("Multipairing equation had a nonzero result.")]
pub struct MultipairingEquationNonZeroResult;
