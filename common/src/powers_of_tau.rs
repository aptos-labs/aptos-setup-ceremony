use ark_ec::{CurveGroup, pairing::Pairing};
use serde::{Deserialize, Serialize};
use aptos_crypto::arkworks::serialization::{ark_de, ark_se};

use crate::contribution::ContributionInner;


pub struct PowersOfTauParams {
    max_power: usize
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PowersOfTauContributionInner<P: Pairing> {
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    powers: Vec<P::G1>,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    tau_g2: P::G2,
}



impl<P: Pairing> ContributionInner for PowersOfTauContributionInner<P> {
    type Params = PowersOfTauParams;

    type Secrets = P::ScalarField;

    type Output = ();

    fn first_contribution(params: &Self::Params) -> Self {
        todo!()
    }

    fn generate<R: rand_core::CryptoRngCore>(rng: &mut R, previous: &Self, params: &Self::Params) -> (Self, Self::Secrets) {
        todo!()
    }

    fn verify(&self, previous: &Self, params: &Self::Params) -> Result<(), crate::errors::ContributionVerificationFailure> {
        todo!()
    }

    fn output(&self) -> Self::Output {
        todo!()
    }
}
