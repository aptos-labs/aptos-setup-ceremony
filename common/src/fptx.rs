use std::ops::Mul;

use ark_ec::hashing::curve_maps::wb::WBMap;
use ark_ec::{AffineRepr, CurveGroup, ScalarMul as _, PrimeGroup};
use ark_ff::UniformRand;
use ark_std::One;
use aptos_batch_encryption::shared::digest::DigestKey;
use aptos_batch_encryption::group::{Fr, G1Affine, G1Projective, G2Affine, G2Projective, Pairing};
use aptos_crypto::arkworks::serialization::{ark_de, ark_se};
use rand::SeedableRng as _;
use rand_core::CryptoRngCore;
use serde::{Deserialize, Serialize};

use crate::batched_schnorr::BatchedSigOfKnowledge;
use crate::bls_sok::BLSSoK;
use crate::contribution::ContributionInner;
use crate::errors::{BatchSizeNotPowerOfTwo, ContributionVerificationFailure};
use crate::multipairing_equation::MultipairingEquation;
use crate::powers_of_tau::{PowersOfTauContributionInner, PowersOfTauParams};

type M2C = WBMap<<G1Projective as CurveGroup>::Config>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FPTXContributionInner {
    pub tau_powers_contrib_inner: PowersOfTauContributionInner<Pairing, M2C>,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    pub alphas_g2: Vec<G2Affine>,
    pub soks_alphas: Vec<BLSSoK<Pairing, M2C>>,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    pub randomized_tau_powers_g1: Vec<Vec<G1Affine>>,

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
    tau_powers_fr: &[Fr],
    random_alphas: &[Fr],
) -> Vec<Vec<Fr>> {
    assert_eq!(params.num_rounds, random_alphas.len());

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

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct HashPreimage<'a>
where
{
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    previous_alphas_g2: &'a [G2Affine],
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    alpha_g2: G2Affine,
    index: usize
}


impl ContributionInner for FPTXContributionInner {
    type P = aptos_batch_encryption::group::Pairing;
    type Params = FPTXParams;
    /// No secret for new b/c we don't need this to be composable
    type Secrets = ();
    type Output = DigestKey;

    fn first_contribution(params: &Self::Params) -> Self {
        let trivial_random_alphas_fr = vec![Fr::one(); params.num_rounds];

        let trivial_random_alphas_g2 : Vec<G2Affine> = trivial_random_alphas_fr.iter()
            .map(|alpha| G2Affine::from(G2Affine::generator() * alpha))
            .collect();

        let tau_powers_trivial_randomness_fr = tau_powers_randomized_fr(
            params, 
            &vec![Fr::one(); params.batch_size], 
            &trivial_random_alphas_fr
        );

        let tau_powers_trivial_randomness_g1: Vec<Vec<G1Affine>> = tau_powers_trivial_randomness_fr
            .into_iter()
            .map(|powers_for_r| G1Projective::from(G1Affine::generator()).batch_mul(&powers_for_r))
            .collect();

        let first_pot_inner = PowersOfTauContributionInner::first_contribution(&PowersOfTauParams { max_power: params.batch_size });

        Self { 
            tau_powers_contrib_inner: first_pot_inner,
            soks_alphas: vec![ BLSSoK::sign(Fr::one(), &String::from("")); trivial_random_alphas_g2.len()],
            alphas_g2: trivial_random_alphas_g2,
            randomized_tau_powers_g1: tau_powers_trivial_randomness_g1,
        }
    }

    fn generate<R: rand_core::CryptoRngCore>(rng: &mut R, previous: &Self, params: &Self::Params) -> (Self, ()) {
        let mut random_alphas_fr = Vec::new();
        for _ in 0..params.num_rounds {
            random_alphas_fr.push(Fr::rand(rng));
        }

        let random_alphas_g2 : Vec<G2Affine> = random_alphas_fr.iter()
            .map(|alpha| G2Affine::from(G2Affine::generator() * alpha))
            .collect();

        let soks_alphas : Vec<BLSSoK<Pairing, M2C>> = random_alphas_fr.iter().enumerate()
            .zip(&random_alphas_g2)
            .map(|((i, alpha_fr), alpha_g2)| BLSSoK::sign(*alpha_fr, &HashPreimage { previous_alphas_g2: &previous.alphas_g2, alpha_g2: *alpha_g2, index: i }))
            .collect();


        let (tau_powers_contrib_inner, tau_powers_fr) = PowersOfTauContributionInner::generate(
            rng, 
            &previous.tau_powers_contrib_inner, 
            &PowersOfTauParams { max_power: params.batch_size }
        );

        let randomized_tau_powers_fr = tau_powers_randomized_fr(
            params, 
            &tau_powers_fr,
            &random_alphas_fr
        );

        let randomized_tau_powers_g1: Vec<Vec<G1Affine>> = randomized_tau_powers_fr
            .into_iter()
            .map(|powers_for_r| G1Projective::from(G1Affine::generator()).batch_mul(&powers_for_r))
            .collect();


        (Self { 
            tau_powers_contrib_inner,
            soks_alphas,
            alphas_g2: random_alphas_g2,
            randomized_tau_powers_g1,
        }, ())
    }

    fn verify(&self, rng: &mut impl CryptoRngCore, previous: &Self, params: &Self::Params) -> Result<MultipairingEquation<Self::P>, ContributionVerificationFailure> {
        if self.alphas_g2.len() != params.num_rounds ||
            self.soks_alphas.len() != params.num_rounds ||
            self.tau_powers_contrib_inner.powers.len() != params.batch_size + 1 ||
            self.randomized_tau_powers_g1.len() != (params.batch_size + 1) * params.num_rounds {
            return Err(ContributionVerificationFailure::ParamsMismatch)
        }

        let pot_equation = self.tau_powers_contrib_inner.verify(rng, &previous.tau_powers_contrib_inner, &PowersOfTauParams { max_power: params.batch_size })?;

        let sok_check_equation_combined = self.alphas_g2.iter().enumerate()
            .zip(&previous.alphas_g2)
            .zip(&self.soks_alphas)
            .map(|(((i, alpha_g2), previous_alpha_g2), sok)| sok.verify(
                G2Projective::from(*previous_alpha_g2), 
                G2Projective::from(*alpha_g2), 
                &HashPreimage {
                    previous_alphas_g2: &previous.alphas_g2,
                    alpha_g2: *alpha_g2,
                    index: i,
                })
            )
            .fold(MultipairingEquation::empty(), |eq1, eq2| eq1.combine(rng, eq2));

        let alpha_check_equation_combined  = self.alphas_g2.iter()
            .zip(&self.randomized_tau_powers_g1)
            .map(|(alpha, tau_powers)| 
                tau_powers.iter().zip(&self.tau_powers_contrib_inner.powers)
                .map(|(randomized_power, nonrandomized_power)| 
                    MultipairingEquation::new(vec![*nonrandomized_power, G1Projective::from(*randomized_power)], vec![G2Projective::generator(), G2Projective::from(*alpha)]))
                .fold(MultipairingEquation::empty(), |eq1, eq2| eq1.combine(rng, eq2)))
            .collect::<Vec<MultipairingEquation<Pairing>>>()
            .into_iter()
            .fold(MultipairingEquation::empty(), |eq1, eq2| eq1.combine(rng, eq2));

        Ok(
            [pot_equation, sok_check_equation_combined, alpha_check_equation_combined]
                .into_iter()
                .fold(MultipairingEquation::empty(), |eq1, eq2| eq1.combine(rng, eq2))
        )

    }

    fn output(&self) -> Self::Output {
        todo!()
    }
}
