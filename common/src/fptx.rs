use ark_std::One;
use aptos_batch_encryption::shared::digest::DigestKey;
use aptos_batch_encryption::group::{Fr, G1Affine, G2Affine};
use aptos_crypto::arkworks::serialization::{ark_de, ark_se};
use serde::{Deserialize, Serialize};

use crate::contribution::ContributionInner;
use crate::errors::BatchSizeNotPowerOfTwo;


#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FPTXContributionInner {
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    pub tau_g2: G2Affine,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    pub random_alphas_g2: Vec<G2Affine>,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    pub tau_powers_g1: Vec<Vec<G1Affine>>,

}

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
    type Params = FPTXParams;
    type Output = DigestKey;

    fn first_contribution(params: &Self::Params) -> Self {
    }

    fn generate<R: rand_core::CryptoRngCore>(rng: &mut R, previous: &Self) -> Self {
        todo!()
    }

    fn verify(&self, previous: &Self) -> Result<(), crate::errors::ContributionVerificationFailure> {
        todo!()
    }

    fn output(&self) -> Self::Output {
        todo!()
    }
}
