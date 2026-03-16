use std::ops::Neg;

use aptos_crypto::arkworks::serialization::{ark_de, ark_se};
use ark_ec::{AffineRepr, hashing::map_to_curve_hasher::MapToCurve, pairing::Pairing};
use serde::{Deserialize, Serialize};

use crate::powers_of_tau::{PowersOfTauContributionInner, PowersOfTauParams};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HidingKZGContributionInner<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G1>,
    P::G2Affine: AffineRepr + Neg<Output = P::G2Affine>,
    P::G1Affine: AffineRepr,
 
{
    pub tau_powers_contrib_inner: PowersOfTauContributionInner<P, M2C>,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    pub xi_g1: P::G1,
    pub xi_g2: P::G1,
}

pub type HidingKZGParams = PowersOfTauParams;

