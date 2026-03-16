use std::ops::Neg;

use aptos_crypto::arkworks::serialization::{ark_de, ark_se};
use crate::parallel_ark_serde::{par_ark_se_vec, par_ark_de_vec};
use ark_ec::{AffineRepr, hashing::map_to_curve_hasher::MapToCurve, pairing::Pairing};
use rand_core::CryptoRngCore;
use serde::{Deserialize, Serialize};
use ark_std::{One, UniformRand};

use crate::{bls_sok::BLSSoK, contribution::ContributionInner, errors::ContributionVerificationFailure, multipairing_equation::{MultipairingEquation, MultipairingEquations}};

pub struct PowersOfTauParams {
    pub max_power: usize,
}
impl PowersOfTauParams {
    pub fn new(max_power: usize) -> Self {
        Self { max_power }
    }
    
}

#[derive(Serialize, Deserialize)]
// otherwise serde adds unneeded P,M2C: Serialize, Deserialize bounds for this struct's
// Serialize, Deserialize impls
#[serde(bound(serialize = "", deserialize = ""))]
pub struct PowersOfTauContributionInner<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G1>,
    P::G2Affine: AffineRepr + Neg<Output = P::G2Affine>,
    P::G1Affine: AffineRepr,
{
    #[serde(serialize_with = "par_ark_se_vec", deserialize_with = "par_ark_de_vec")]
    pub(super) powers: Vec<P::G1Affine>,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    pub(super) tau_g2: P::G2Affine,
    pub(super) sok: BLSSoK<P, M2C>,
}


#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct HashPreimage<P>
where
    P: Pairing,
{
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    previous_tau_g2: P::G2Affine,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    powers: Vec<P::G1Affine>,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    tau_g2: P::G2Affine,
}

impl<P, M2C> ContributionInner for PowersOfTauContributionInner<P, M2C>
where
    P: Pairing,
    P::G2Affine: AffineRepr + Neg<Output = P::G2Affine>,
    P::G1Affine: AffineRepr,
    M2C: MapToCurve<P::G1>,
{
    type P = P;

    type Params = PowersOfTauParams;

    /// The secret is simply tau. We return every power so that downstream we avoid
    /// recomputing.
    type Secrets = Vec<P::ScalarField>;

    /// No output b/c this should be a generic/composable PoT inner contribution, not one specific
    /// to a construction
    type Output = ();

    fn first_contribution(params: &Self::Params) -> Self {
        Self {
            powers: vec![P::G1Affine::generator(); params.max_power + 1],
            tau_g2: P::G2Affine::generator(),
            sok: BLSSoK::sign(P::ScalarField::one(), &String::from(""))
        }
    }

    fn generate<R: rand_core::CryptoRngCore>(
        rng: &mut R,
        previous: &Self,
        params: &Self::Params,
    ) -> (Self, Self::Secrets) {
        let current_contribution_tau_fr = P::ScalarField::rand(rng);
        let current_contribution_tau_powers_fr : Vec<P::ScalarField> = std::iter::successors(
            Some(P::ScalarField::one()),
            |power| Some(*power * current_contribution_tau_fr))
            .take(params.max_power + 1)
            .collect();

        let new_powers : Vec<P::G1Affine> = current_contribution_tau_powers_fr
            .iter()
            .zip(&previous.powers)
            .map(|(current_tau_fr, previous_power_g)| P::G1Affine::from(*previous_power_g * current_tau_fr))
            .collect();

        let new_tau_g2 = P::G2Affine::from(previous.tau_g2 * current_contribution_tau_fr);

        (
            Self {
                powers: new_powers.clone(),
                tau_g2: new_tau_g2,
                sok: BLSSoK::sign(current_contribution_tau_fr, &HashPreimage::<P> { previous_tau_g2: previous.tau_g2, powers: new_powers, tau_g2: new_tau_g2 }),
            }, 
            current_contribution_tau_powers_fr
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

        let powers_check_equation_combined = self.powers.iter().skip(1)
            .zip(&self.powers)
            .map(|(higher, lower)| MultipairingEquation::simple(vec![*higher, *lower], vec![P::G2Affine::generator(), -self.tau_g2]))
            .fold(MultipairingEquations::new(), |eqs, eq2| eqs.add(eq2))
            .compact(rng);

        Ok(sok_check_equation.combine(rng, powers_check_equation_combined))
    }

    fn output(&self) -> Self::Output {
        ()
    }
}

#[cfg(test)]
mod tests {

    use ark_ec::{CurveGroup, hashing::curve_maps::wb::WBMap};
    use rand::thread_rng;

    use crate::powers_of_tau::PowersOfTauContributionInner;
    use crate::contribution::ContributionInner;

    use aptos_batch_encryption::group::{G1Projective, Pairing};
    type M2C = WBMap<<G1Projective as CurveGroup>::Config>;


    #[test]
    fn test_pot_contribute() {
        let mut rng = thread_rng();
        let params = super::PowersOfTauParams::new(5);

        let first_contrib : PowersOfTauContributionInner<Pairing, M2C> = PowersOfTauContributionInner::first_contribution(&params);
        let (new_contrib, _) = PowersOfTauContributionInner::generate(&mut rng, &first_contrib, &params);

        new_contrib.verify(&mut rng, &first_contrib, &params)
            .unwrap()
            .equals_zero()
            .unwrap();
    }
}
