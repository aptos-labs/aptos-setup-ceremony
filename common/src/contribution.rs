use ark_ec::pairing::Pairing;
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand_core::CryptoRngCore;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tabled::Tabled;

use crate::{errors::{ContributionVerificationFailure, DeserializationError}, multipairing_equation::MultipairingEquation};

// TODO switch this file to use SHA2 instead of blake3

#[derive(Tabled, Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct Contributor {
    pub name: String,
    pub email: String,
    #[tabled(format("{:?}", hex::encode(self.verifying_key.as_bytes())))]
    pub verifying_key: VerifyingKey,
}

impl Ord for Contributor {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.name.cmp(&other.name) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        self.email.cmp(&other.email)
    }
}

impl PartialOrd for Contributor {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
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

    pub fn new_with_verifying_key(
        name: &str, 
        email: &str,
        verifying_key: VerifyingKey,
    ) -> Self {
        Self {
            name: name.into(),
            email: email.into(),
            verifying_key,
        }
    }

}

pub trait AsAndFromHex: Sized {
    fn as_hex(&self) -> anyhow::Result<String>;
    fn from_hex(hex: &str) -> anyhow::Result<Self>;
}

impl AsAndFromHex for (SigningKey, Contributor) {
    fn as_hex(&self) -> anyhow::Result<String> {
        Ok(hex::encode(bcs::to_bytes(&self)?))
    }
    fn from_hex(hex: &str) -> anyhow::Result<Self> {
        Ok(bcs::from_bytes(&hex::decode(hex)?)?)

    }
}

pub trait ContributionInner : Serialize + DeserializeOwned {
    /// The pairing over which this inner contribution is defined.
    type P : Pairing;
    /// The params required for initializing the ceremony.
    type Params;
    /// The secrets that the current contribution used. This is output during generate, for better
    /// composition.
    type Secrets;
    /// The type of the result of the ceremony
    type Output : Eq + PartialEq;
    /// Fixed, first "dummy" inner contribution. For instance, a powers of "tau" where tau = [1].
    fn first_contribution(params: &Self::Params) -> Self;
    /// Compute an inner contribution w.r.t. a previous inner contribution.
    fn generate<R: CryptoRngCore>(rng: &mut R, previous: &Self, params: &Self::Params) -> (Self, Self::Secrets);
    /// Verify this inner contribution w.r.t. a previous inner contribution. This returns a 
    /// [`MultipairingEquation`]; the verification is considered to pass iff this equation
    /// evaluates to zero. The reason for returning this equation instead of evaluating directly
    /// within `verify` is so that the equation verifications can be batched in a single
    /// multipairing when composing multiple inner contributions.
    fn verify(&self, rng: &mut impl CryptoRngCore, previous: &Self, params: &Self::Params) -> Result<MultipairingEquation<Self::P>, ContributionVerificationFailure>;
    /// Output the ceremony result. Note that this is deterministic; given a final contribution and
    /// an output, we want to be able to verify the output by recomputing and testing for equality.
    fn output(self) -> Self::Output;
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
        maybe_previous: Option<&Self>, 
        current_contributor: &Contributor,
        params: &C::Params,
    ) -> Self {
        let (inner, previous_hashes) = if let Some(previous) = maybe_previous {
            let previous_hashes = Self::build_previous_hashes(previous);

            (
                C::generate(rng, &previous.inner, params).0,
                previous_hashes
            )
        } else {
            (
                C::generate(rng, &C::first_contribution(params), params).0,
                Vec::new()
            )
        };

        Self {
            inner,
            contributor: current_contributor.clone(),
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
        bcs::from_bytes(bytes).map_err(|e| DeserializationError(e))
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

    pub fn verify(&self, rng: &mut impl CryptoRngCore, maybe_previous: Option<&Self>, params: &C::Params) -> Result<(), ContributionVerificationFailure> {
        if let Some(previous) = maybe_previous {
            if self.previous_hashes != Self::build_previous_hashes(previous) {
                Err(ContributionVerificationFailure::ContributionHashMismatch)
            } else {
                self.inner.verify(rng, &previous.inner, params)?.equals_zero()
                    .map_err(|_| ContributionVerificationFailure::MultipairingEquationNonZeroResult)
            }
        } else {
            self.inner.verify(rng, &C::first_contribution(params), params)?.equals_zero()
            .map_err(|_| ContributionVerificationFailure::MultipairingEquationNonZeroResult)
        }
    }

    pub fn output(self) -> C::Output {
        self.inner.output()
    }
}

#[cfg(test)]
mod tests {
    use rand::thread_rng;

    use super::Contribution;
    use crate::{contribution::Contributor, fptx::{FPTXContributionInner, FPTXParams}};

    #[test]
    fn test_contribute() {
        let mut rng = thread_rng();
        let params = FPTXParams::new(8, 4).unwrap();
        
        let (_, current_contributor) = Contributor::new("Asdf Asdf", "asdf@asdf.com", &mut rng);

        let new_contrib = Contribution::<FPTXContributionInner>::generate(&mut rng, None, &current_contributor, &params);

        new_contrib.verify(&mut rng, None, &params)
            .unwrap();
    }

    #[test]
    fn test_contribute_2() {
        let mut rng = thread_rng();
        let params = FPTXParams::new(8, 4).unwrap();

        let (_, first_contributor) = Contributor::new("Asdf Asdf", "asdf@asdf.com", &mut rng);
        let (_, second_contributor) = Contributor::new("Hjkl Hjkl", "Hjkl@asdf.com", &mut rng);

        let first = Contribution::<FPTXContributionInner>::generate(&mut rng, None, &first_contributor, &params);

        first.verify(&mut rng, None, &params)
            .unwrap();

        let second = Contribution::<FPTXContributionInner>::generate(&mut rng, Some(&first), &second_contributor, &params);

        second.verify(&mut rng, Some(&first), &params)
            .unwrap();

        println!("{:?}", second.contributor());
        println!("{:?}", second.previous_hashes);

    }
}
