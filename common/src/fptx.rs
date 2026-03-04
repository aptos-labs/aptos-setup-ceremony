use ark_ec::hashing::curve_maps::wb::WBMap;
use ark_ec::{AffineRepr, CurveGroup, ScalarMul as _};
use ark_std::One;
use aptos_batch_encryption::shared::digest::DigestKey;
use aptos_batch_encryption::group::{Fr, G1Affine, G1Projective, G2Affine, G2Projective, Pairing};
use aptos_crypto::arkworks::serialization::{ark_de, ark_se};
use rand::SeedableRng as _;
use rand_core::CryptoRngCore;
use serde::{Deserialize, Serialize};

use crate::batched_schnorr::BatchedSigOfKnowledge;
use crate::contribution::ContributionInner;
use crate::errors::{BatchSizeNotPowerOfTwo, ContributionVerificationFailure};
use crate::multipairing_equation::MultipairingEquation;
use crate::powers_of_tau::{PowersOfTauContributionInner, PowersOfTauParams};

type M2C = WBMap<<G1Projective as CurveGroup>::Config>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FPTXContributionInner {
    pub tau_powers_contrib_inner: PowersOfTauContributionInner<Pairing, M2C>,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    pub random_alphas_g2: Vec<G2Affine>,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    pub tau_powers_g1: Vec<Vec<G1Affine>>,
    pub sok: BatchedSigOfKnowledge<G2Projective>,

}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FPTXParams {
    batch_size: usize,
    num_rounds: usize,
}

impl FPTXParams {
    pub fn new(batch_size: usize, num_rounds: usize) -> Result<Self,BatchSizeNotPowerOfTwo> {
        let mut i = batch_size;
        while i > 1 {
            (i % 2 == 0)
                .then_some(())
                .ok_or(BatchSizeNotPowerOfTwo)?;
            i >>= 1;
        }
        Ok(Self { batch_size, num_rounds })
    }

}

fn tau_powers_randomized_fr(
    params: &FPTXParams,
    tau: Fr,
    random_alphas: &[Fr],
) -> Vec<Vec<Fr>> {
    assert_eq!(params.num_rounds, random_alphas.len());

    let mut tau_powers_fr = vec![Fr::one()];
    let mut cur = tau;
    for _ in 0..params.batch_size {
        tau_powers_fr.push(cur);
        cur *= &tau;
    }


    let tau_powers_randomized_fr = random_alphas
        .into_iter()
        .map(|alpha| {
            tau_powers_fr
                .iter()
                .map(|tau_power| alpha * tau_power)
                .collect::<Vec<Fr>>()
        })
        .collect::<Vec<Vec<Fr>>>();

    tau_powers_randomized_fr
}

impl ContributionInner for FPTXContributionInner {
    type P = aptos_batch_encryption::group::Pairing;
    type Params = FPTXParams;
    type Secrets = ();
    type Output = DigestKey;

    fn first_contribution(params: &Self::Params) -> Self {
        let trivial_random_alphas_fr = vec![Fr::one(); params.num_rounds];

        let tau_powers_trivial_randomness_fr = tau_powers_randomized_fr(
            params, 
            Fr::one(), 
            &trivial_random_alphas_fr
        );

        let trivial_random_alphas_g2 : Vec<G2Affine> = trivial_random_alphas_fr.iter()
            .map(|alpha| G2Affine::from(G2Affine::generator() * alpha))
            .collect();


        let tau_powers_trivial_randomness_g1: Vec<Vec<G1Affine>> = tau_powers_trivial_randomness_fr
            .into_iter()
            .map(|powers_for_r| G1Projective::from(G1Affine::generator()).batch_mul(&powers_for_r))
            .collect();

        let mut rng = rand::rngs::StdRng::from_seed([0u8; 32]);


        let mut target_elts = vec![G2Affine::generator()]; target_elts.extend_from_slice(&trivial_random_alphas_g2);
        let mut secret_exponents = vec![Fr::one()]; secret_exponents.extend_from_slice(&trivial_random_alphas_fr);
        let sok = BatchedSigOfKnowledge::sign(
            &mut rng,
            &target_elts,
            &secret_exponents, 
            &String::new(),
        );

        let first_pot_inner = PowersOfTauContributionInner::first_contribution(&PowersOfTauParams { max_power: params.batch_size });

        Self { 
            tau_powers_contrib_inner: first_pot_inner,
            random_alphas_g2: trivial_random_alphas_g2,
            tau_powers_g1: tau_powers_trivial_randomness_g1,
            sok
        }

    }

    fn generate<R: rand_core::CryptoRngCore>(_rng: &mut R, _previous: &Self, _params: &Self::Params) -> (Self, ()) {
       todo!() 
    }

    fn verify(&self, rng: &mut impl CryptoRngCore, _previous: &Self, _params: &Self::Params) -> Result<MultipairingEquation<Self::P>, ContributionVerificationFailure> {
        todo!()
    }

    fn output(&self) -> Self::Output {
        todo!()
    }
}
