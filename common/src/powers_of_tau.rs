use aptos_crypto::arkworks::serialization::{ark_de, ark_se};
use ark_ec::{CurveGroup, PrimeGroup, hashing::map_to_curve_hasher::MapToCurve, pairing::Pairing};
use serde::{Deserialize, Serialize};

use crate::{bls_sok::BLSSoK, contribution::ContributionInner, errors::ContributionVerificationFailure, multipairing_equation::MultipairingEquation};

pub struct PowersOfTauParams {
    max_power: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PowersOfTauContributionInner<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G2>,
    P::G1: CurveGroup,
    P::G2: CurveGroup,
{
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    powers: Vec<P::G1>,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    tau_g2: P::G2,
    sok: BLSSoK<P, M2C>,
}

impl<P, M2C> ContributionInner for PowersOfTauContributionInner<P, M2C>
where
    P: Pairing,
    P::G1: CurveGroup,
    P::G2: CurveGroup,
    M2C: MapToCurve<P::G2>,
{
    type P = P;

    type Params = PowersOfTauParams;

    type Secrets = P::ScalarField;

    type Output = ();

    fn first_contribution(params: &Self::Params) -> Self {
        Self {
            powers: vec![P::G1::generator(); params.max_power + 1],
            tau_g2: P::G2::generator()
        }
    }

    fn generate<R: rand_core::CryptoRngCore>(
        _rng: &mut R,
        _previous: &Self,
        _params: &Self::Params,
    ) -> (Self, Self::Secrets) {
        todo!()
    }

    fn verify(
        &self,
        _previous: &Self,
        _params: &Self::Params,
    ) -> Result<MultipairingEquation<P>, ContributionVerificationFailure> {
        todo!()
    }

    fn output(&self) -> Self::Output {
        todo!()
    }
}
