use aptos_crypto::arkworks::serialization::{ark_de, ark_se};
use ark_ec::{CurveGroup, PrimeGroup, hashing::map_to_curve_hasher::MapToCurve, pairing::Pairing};
use rand_core::CryptoRngCore;
use serde::{Deserialize, Serialize};
use ark_std::{One, UniformRand};

use crate::{bls_sok::BLSSoK, contribution::ContributionInner, errors::ContributionVerificationFailure, multipairing_equation::MultipairingEquation};

pub struct PowersOfTauParams {
    max_power: usize,
}

#[derive(Serialize, Deserialize)]
// otherwise serde adds unneeded P,M2C: Serialize, Deserialize bounds for this struct's
// Serialize, Deserialize impls
#[serde(bound(serialize = "", deserialize = ""))]
pub struct PowersOfTauContributionInner<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G1>,
    P::G1: CurveGroup,
    P::G2: CurveGroup,
{
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    powers: Vec<P::G1>,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    tau_g2: P::G2,
    sok: BLSSoK<P, M2C>,
}

impl<P: Eq, M2C> Eq for PowersOfTauContributionInner<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G1>,
    P::G1: CurveGroup,
    P::G2: CurveGroup,
{
}

impl<P: PartialEq, M2C> PartialEq for PowersOfTauContributionInner<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G1>,
    P::G1: CurveGroup,
    P::G2: CurveGroup,
{
    fn eq(&self, other: &Self) -> bool {
        self.powers == other.powers && self.tau_g2 == other.tau_g2 && self.sok == other.sok
    }
}

impl<P: Clone, M2C> Clone for PowersOfTauContributionInner<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G1>,
    P::G1: CurveGroup,
    P::G2: CurveGroup,
{
    fn clone(&self) -> Self {
        Self { powers: self.powers.clone(), tau_g2: self.tau_g2.clone(), sok: self.sok.clone() }
    }
}

impl<P: std::fmt::Debug, M2C> std::fmt::Debug for PowersOfTauContributionInner<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G1>,
    P::G1: CurveGroup,
    P::G2: CurveGroup,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PowersOfTauContributionInner").field("powers", &self.powers).field("tau_g2", &self.tau_g2).field("sok", &self.sok).finish()
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct HashPreimage<P>
where
    P: Pairing,
{
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    previous_tau_g2: P::G2,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    powers: Vec<P::G1>,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    tau_g2: P::G2,
}

impl<P, M2C> ContributionInner for PowersOfTauContributionInner<P, M2C>
where
    P: Pairing,
    P::G1: CurveGroup,
    P::G2: CurveGroup,
    M2C: MapToCurve<P::G1>,
{
    type P = P;

    type Params = PowersOfTauParams;

    /// The secret is simply tau
    type Secrets = P::ScalarField;

    /// No output b/c this should be a generic/composable PoT inner contribution, not one specific
    /// to a construction
    type Output = ();

    fn first_contribution(params: &Self::Params) -> Self {
        Self {
            powers: vec![P::G1::generator(); params.max_power + 1],
            tau_g2: P::G2::generator(),
            sok: BLSSoK::sign(P::ScalarField::one(), &String::from(""))
        }
    }

    fn generate<R: rand_core::CryptoRngCore>(
        rng: &mut R,
        previous: &Self,
        params: &Self::Params,
    ) -> (Self, Self::Secrets) {
        let current_contribution_tau_fr = P::ScalarField::rand(rng);
        let current_contribution_tau_powers_fr = std::iter::successors(
            Some(P::ScalarField::one()),
            |power| Some(*power * current_contribution_tau_fr))
            .take(params.max_power + 1);

        let new_powers : Vec<P::G1> = current_contribution_tau_powers_fr
            .zip(&previous.powers)
            .map(|(current_tau_fr, previous_power_g)| *previous_power_g * current_tau_fr)
            .collect();

        let new_tau_g2 = previous.tau_g2 * current_contribution_tau_fr;

        (
            Self {
                powers: new_powers.clone(),
                tau_g2: new_tau_g2,
                sok: BLSSoK::sign(current_contribution_tau_fr, &HashPreimage::<P> { previous_tau_g2: previous.tau_g2, powers: new_powers, tau_g2: new_tau_g2 }),
            }, 
            current_contribution_tau_fr
        )

    }

    fn verify(
        &self,
        rng: &mut impl CryptoRngCore,
        previous: &Self,
        params: &Self::Params,
    ) -> Result<MultipairingEquation<P>, ContributionVerificationFailure> {
        if self.powers.len() != params.max_power + 1 {
            return Err(ContributionVerificationFailure::ParamsMismatch);
        }

        let sok_check_equation = self.sok.verify(
            previous.tau_g2,
            self.tau_g2,
            &HashPreimage::<P> { previous_tau_g2: previous.tau_g2, powers: self.powers.clone(), tau_g2: self.tau_g2 }
        );

        let powers_check_equations = self.powers.iter().skip(1)
            .zip(&self.powers)
            .map(|(higher, lower)| MultipairingEquation::new(vec![*higher, *lower], vec![P::G2::generator(), self.tau_g2]))
            .fold(MultipairingEquation::empty(), |eq1, eq2| eq1.combine(rng, eq2));
                
        Ok(sok_check_equation.combine(rng, powers_check_equations))
    }

    fn output(&self) -> Self::Output {
        ()
    }
}
