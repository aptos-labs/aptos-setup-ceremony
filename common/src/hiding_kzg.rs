use std::{ops::Neg, time::Instant};

use aptos_crypto::arkworks::serialization::{ark_de, ark_se};
use aptos_dkg::pcs::univariate_hiding_kzg;
use ark_ec::{AffineRepr, hashing::map_to_curve_hasher::MapToCurve, pairing::Pairing};
use rand_core::CryptoRngCore;
use serde::{Deserialize, Serialize};
use ark_std::{One, UniformRand};

use crate::{bls_sok::BLSSoK, contribution::ContributionInner, errors::{ContributionGenerationFailure, ContributionVerificationFailure}, multipairing_equation::{MultipairingEquation, MultipairingEquations}, powers_of_tau::{PowersOfTauContributionInner, PowersOfTauParams}};

#[derive(Serialize, Deserialize)]
// otherwise serde adds unneeded P,M2C: Serialize, Deserialize bounds for this struct's
// Serialize, Deserialize impls
#[serde(bound(serialize = "", deserialize = ""))]
pub struct HidingKZGContributionInner<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G1>,
    P::G2Affine: AffineRepr + Neg<Output = P::G2Affine>,
    P::G1Affine: AffineRepr,
 
{
    pub tau_powers_contrib_inner: PowersOfTauContributionInner<P, M2C>,
    // The base element by which commitments are randomized, in both G1
    // and G2. We use \xi for this in order to have naming consistent with
    // the naming in `aptos-dkg`.
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    pub xi_g1: P::G1Affine,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    pub xi_g2: P::G2Affine,
    pub(super) sok_xi: BLSSoK<P, M2C>,
}

pub type HidingKZGParams = PowersOfTauParams;


#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct HashPreimage<P>
where
    P: Pairing,
    P::G2Affine: AffineRepr + Neg<Output = P::G2Affine>,
    P::G1Affine: AffineRepr,
{

    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    previous_xi_g1: P::G1Affine,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    previous_xi_g2: P::G2Affine,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    new_xi_g2: P::G2Affine,
}

impl<P, M2C> ContributionInner for HidingKZGContributionInner<P, M2C>
where
    P: Pairing,
    P::G2Affine: AffineRepr + Neg<Output = P::G2Affine>,
    P::G1Affine: AffineRepr,
    M2C: MapToCurve<P::G1>,
{
    type P = P;
    type Params = HidingKZGParams;

    type Output = (univariate_hiding_kzg::VerificationKey<P>, univariate_hiding_kzg::CommitmentKey<P>);

    // The secret consists of the powers of tau secret along with the scalar xi.
    type Secrets = (<PowersOfTauContributionInner<P, M2C> as ContributionInner>::Secrets, P::ScalarField);

    fn first_contribution(params: &Self::Params) -> Self {
        Self {
            tau_powers_contrib_inner: PowersOfTauContributionInner::first_contribution(params),
            xi_g1: P::G1Affine::generator(),
            xi_g2: P::G2Affine::generator(),
            sok_xi: BLSSoK::sign(P::ScalarField::one(), &String::from(""))
        }
    }

    fn generate<R: CryptoRngCore>(
        rng: &mut R,
        previous: &Self,
        params: &Self::Params,
    ) -> Result<(Self, Self::Secrets), ContributionGenerationFailure> {
        if previous.tau_powers_contrib_inner.powers.len() != params.max_power + 1 {
            return Err(ContributionGenerationFailure::ParamsMismatch);
        }

        let start = Instant::now();

        let (tau_powers_contrib_inner, tau_powers_fr) = PowersOfTauContributionInner::generate(
            rng, 
            &previous.tau_powers_contrib_inner, 
            params
        )?;

        let current_contribution_xi_fr = P::ScalarField::rand(rng);
        let new_xi_g1 = P::G1Affine::from(previous.xi_g1 * current_contribution_xi_fr);
        let new_xi_g2 = P::G2Affine::from(previous.xi_g2 * current_contribution_xi_fr);

        let sok_xi =  BLSSoK::sign(
            current_contribution_xi_fr, 
            &HashPreimage::<P> {
                previous_xi_g1: previous.xi_g1,
                previous_xi_g2: previous.xi_g2,
                new_xi_g2,
            }
        );

        print!("Finished hiding KZG contrib in {:?}", start.elapsed());

        Ok((
            Self {
                tau_powers_contrib_inner,
                xi_g1: new_xi_g1,
                xi_g2: new_xi_g2,
                sok_xi,
            },
            (tau_powers_fr, current_contribution_xi_fr)
        ))
    }


    fn verify(
        &self,
        rng: &mut impl CryptoRngCore,
        previous: &Self,
        params: &Self::Params,
    ) -> Result<MultipairingEquation<P>, ContributionVerificationFailure> {
        if self.tau_powers_contrib_inner.powers.len() != params.max_power + 1 {
            return Err(ContributionVerificationFailure::ParamsMismatch);
        }

        let pot_equation = self.tau_powers_contrib_inner.verify(rng, &previous.tau_powers_contrib_inner, params)?;
        
        let sok_check_equation = self.sok_xi.verify(
            previous.xi_g2,
            self.xi_g2,
            &HashPreimage::<P> { previous_xi_g1: previous.xi_g1, previous_xi_g2: previous.xi_g2, new_xi_g2: self.xi_g2 }
        );

        let xi_g1_g2_consistency_equation = 
        MultipairingEquation::simple(
            vec![ self.xi_g1,                P::G1Affine::generator() ],
            vec![ -P::G2Affine::generator(), self.xi_g2               ],
        );

        Ok(
            MultipairingEquations::new()
                .add(pot_equation)
                .add(sok_check_equation)
                .add(xi_g1_g2_consistency_equation)
                .compact(rng)
        )
    }

    fn output(self) -> Self::Output {
        univariate_hiding_kzg::setup(&self.tau_powers_contrib_inner.powers, self.tau_powers_contrib_inner.tau_g2, self.xi_g1, self.xi_g2)
    }
}

#[cfg(test)]
mod tests {
    use ark_ec::{AffineRepr, CurveGroup, hashing::curve_maps::wb::WBMap};
    use rand::thread_rng;

    use crate::{contribution::ContributionInner, hiding_kzg::{HidingKZGContributionInner, HidingKZGParams}};
    use aptos_batch_encryption::group::{G1Affine, G2Affine, Pairing, G1Projective};

    type M2C = WBMap<<G1Projective as CurveGroup>::Config>;

    #[test]
    fn test_hkzg_contribute() {
        let mut rng = thread_rng();
        let params = HidingKZGParams::new(8);

        let first_contrib : HidingKZGContributionInner<Pairing, M2C> =  HidingKZGContributionInner::first_contribution(&params);
        let (new_contrib, _) = HidingKZGContributionInner::generate(&mut rng, &first_contrib, &params).unwrap();

        new_contrib.verify(&mut rng, &first_contrib, &params)
            .unwrap()
            .equals_zero()
            .unwrap();
    }

    #[test]
    fn test_hkzg_contribute_2() {
        let mut rng = thread_rng();
        let params = HidingKZGParams::new(8);

        let first_contrib : HidingKZGContributionInner<Pairing, M2C> = HidingKZGContributionInner::first_contribution(&params);
        let (new_contrib, _) = HidingKZGContributionInner::generate(&mut rng, &first_contrib, &params).unwrap();

        new_contrib.verify(&mut rng, &first_contrib, &params)
            .unwrap()
            .equals_zero()
            .unwrap();

        let (new_contrib_2, _) = HidingKZGContributionInner::generate(&mut rng, &new_contrib, &params).unwrap();

        new_contrib_2.verify(&mut rng, &new_contrib, &params)
            .unwrap()
            .equals_zero()
            .unwrap();
    }

    #[test]
    #[should_panic]
    fn test_hkzg_contribute_invalid() {
        let mut rng = thread_rng();
        let params = HidingKZGParams::new(8);

        let first_contrib : HidingKZGContributionInner<Pairing, M2C> = HidingKZGContributionInner::first_contribution(&params);
        let (mut new_contrib, _) = HidingKZGContributionInner::generate(&mut rng, &first_contrib, &params).unwrap();

        new_contrib.sok_xi.sig = G1Affine::from(new_contrib.sok_xi.sig + G1Affine::generator());

        new_contrib.verify(&mut rng, &first_contrib, &params)
            .unwrap()
            .equals_zero()
            .unwrap();
    }

    #[test]
    #[should_panic]
    fn test_hkzg_contribute_invalid_2() {
        let mut rng = thread_rng();
        let params = HidingKZGParams::new(8);

        let first_contrib : HidingKZGContributionInner<Pairing, M2C> = HidingKZGContributionInner::first_contribution(&params);
        let (mut new_contrib, _) = HidingKZGContributionInner::generate(&mut rng, &first_contrib, &params).unwrap();

        new_contrib.xi_g2 = G2Affine::from(new_contrib.xi_g2 + G2Affine::generator());

        new_contrib.verify(&mut rng, &first_contrib, &params)
            .unwrap()
            .equals_zero()
            .unwrap();
    }

    #[test]
    #[should_panic]
    fn test_hkzg_contribute_invalid_3() {
        let mut rng = thread_rng();
        let params = HidingKZGParams::new(8);

        let first_contrib : HidingKZGContributionInner<Pairing, M2C> = HidingKZGContributionInner::first_contribution(&params);
        let (mut new_contrib, _) = HidingKZGContributionInner::generate(&mut rng, &first_contrib, &params).unwrap();

        new_contrib.xi_g1 = G1Affine::from(new_contrib.xi_g1 + G1Affine::generator());

        new_contrib.verify(&mut rng, &first_contrib, &params)
            .unwrap()
            .equals_zero()
            .unwrap();
    }

    #[test]
    #[should_panic]
    fn test_hkzg_contribute_invalid_4() {
        let mut rng = thread_rng();
        let params = HidingKZGParams::new(8);

        let first_contrib : HidingKZGContributionInner<Pairing, M2C> = HidingKZGContributionInner::first_contribution(&params);
        let (mut new_contrib, _) = HidingKZGContributionInner::generate(&mut rng, &first_contrib, &params).unwrap();

        new_contrib.tau_powers_contrib_inner.tau_g2 = G2Affine::from(new_contrib.tau_powers_contrib_inner.tau_g2 + G2Affine::generator());

        new_contrib.verify(&mut rng, &first_contrib, &params)
            .unwrap()
            .equals_zero()
            .unwrap();
    }

    #[test]
    #[should_panic]
    fn test_hkzg_contribute_invalid_5() {
        let mut rng = thread_rng();
        let params = HidingKZGParams::new(8);

        let first_contrib : HidingKZGContributionInner<Pairing, M2C> = HidingKZGContributionInner::first_contribution(&params);
        let (mut new_contrib, _) = HidingKZGContributionInner::generate(&mut rng, &first_contrib, &params).unwrap();

        new_contrib.tau_powers_contrib_inner.powers[0] = G1Affine::from(new_contrib.tau_powers_contrib_inner.powers[0] + G1Affine::generator());

        new_contrib.verify(&mut rng, &first_contrib, &params)
            .unwrap()
            .equals_zero()
            .unwrap();
    }

    #[test]
    #[should_panic]
    fn test_hkzg_contribute_invalid_6() {
        let mut rng = thread_rng();
        let params = HidingKZGParams::new(8);

        let first_contrib : HidingKZGContributionInner<Pairing, M2C> = HidingKZGContributionInner::first_contribution(&params);
        let (mut new_contrib, _) = HidingKZGContributionInner::generate(&mut rng, &first_contrib, &params).unwrap();

        new_contrib.tau_powers_contrib_inner.sok.sig = G1Affine::from(new_contrib.tau_powers_contrib_inner.sok.sig + G1Affine::generator());

        new_contrib.verify(&mut rng, &first_contrib, &params)
            .unwrap()
            .equals_zero()
            .unwrap();
    }
}
