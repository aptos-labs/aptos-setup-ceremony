use ed25519_dalek::{SigningKey, VerifyingKey};
use rand_core::CryptoRngCore;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::errors::{ContributionVerificationFailure, DeserializationError};

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct Contributor {
    pub name: String,
    pub email: String,
    // note: currently verifying key is unused. Plan to use it later when implementing the queue
    // manager, to authenticate all messages sent. Could potentially include a signature in
    // [`Contribution`] itself; need to think through benefits/drawbacks
    pub verifying_key: VerifyingKey,
}

impl Contributor {
    pub fn new(
        name: &str, 
        email: &str,
        rng: &mut impl CryptoRngCore,
    ) -> (SigningKey, Self) {
        let signing_key = SigningKey::generate(rng);
        let verifying_key = signing_key.verifying_key();
        (signing_key, Self {
            name: name.into(),
            email: email.into(),
            verifying_key,
        })
    }
}

pub trait ContributionInner : Serialize + DeserializeOwned {
    /// The params required for initializing the ceremony.
    type Params;
    /// The type of the result of the ceremony
    type Output : Eq + PartialEq;
    /// Fixed, first "dummy" inner contribution. For instance, a powers of "tau" where tau = [1].
    fn first_contribution(params: &Self::Params) -> Self;
    /// Compute an inner contribution w.r.t. a previous inner contribution.
    fn generate<R: CryptoRngCore>(rng: &mut R, previous: &Self) -> Self;
    /// Verify this inner contribution w.r.t. a previous inner contribution.
    fn verify(&self, previous: &Self) -> Result<(), ContributionVerificationFailure>;
    /// Output the ceremony result. Note that this is deterministic; given a final contribution and
    /// an output, we want to be able to verify the output by recomputing and testing for equality.
    fn output(&self) -> Self::Output;
}


/// Separate from [`Contribution`] for type safety: we don't want to allow generating an output
/// from the dummy contribution.
#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq)]
#[serde(bound(deserialize = "C: DeserializeOwned"))]
pub struct Contribution<C: ContributionInner> {
    inner: C,
    contributor: Contributor,
    previous_hashes: Vec<(Contributor, blake3::Hash)>,
}

impl<C: ContributionInner> Contribution<C> {
    /// Compute a contribution. Optionally takes a previous contribution; if none is given,
    /// computes the first contribution of a ceremony. 
    pub fn generate<R: CryptoRngCore>(
        rng: &mut R, 
        maybe_previous: &Option<Self>, 
        current_contributor: Contributor,
        params: &C::Params,
    ) -> Self {
        let (inner, previous_hashes) = if let Some(previous) = maybe_previous {
            let previous_hashes = Self::build_previous_hashes(&previous);

            (
                C::generate(rng, &previous.inner),
                previous_hashes
            )
        } else {
            (
                C::generate(rng, &C::first_contribution(params)),
                Vec::new()
            )
        };

        Self {
            inner,
            contributor: current_contributor,
            previous_hashes
        }
    }

    fn build_previous_hashes(previous: &Self) -> Vec<(Contributor, blake3::Hash)> {
        let mut previous_hashes = previous.previous_hashes.clone();
        previous_hashes.push((previous.contributor.clone(), previous.hash()));
        previous_hashes
    }

    pub fn contributor(&self) -> &Contributor {
        &self.contributor
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DeserializationError> {
        bcs::from_bytes(bytes)
        .or_else(|e| Err(DeserializationError(e)))
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        bcs::to_bytes(&self)
            .expect("BCS should never fail to serialize (see failure conditions in bcs docs)")
    }

    pub fn hash(&self) -> blake3::Hash {
        blake3::hash(&self.as_bytes())
    }

    /// In-order list of previous `(Contributor, Hash)` pairs
    pub fn previous_hashes(&self) -> &[(Contributor, blake3::Hash)] {
        &self.previous_hashes
    }

    pub fn verify(&self, previous: &Self) -> Result<(), ContributionVerificationFailure> {
        if self.previous_hashes != Self::build_previous_hashes(previous) {
            Err(ContributionVerificationFailure::ContributionHashMismatch)
        } else {
            self.inner.verify(&previous.inner)
        }
    }

    pub fn output(&self) -> C::Output {
        self.inner.output()
    }
}
