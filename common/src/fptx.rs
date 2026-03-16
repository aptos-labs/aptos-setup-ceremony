
use ark_ec::hashing::curve_maps::wb::WBMap;
use ark_ec::{AffineRepr, CurveGroup, ScalarMul as _};
use ark_ff::UniformRand;
use ark_std::One;
use aptos_batch_encryption::shared::digest::DigestKey;
use aptos_batch_encryption::group::{Fr, G1Affine, G1Projective, G2Affine, Pairing};
use aptos_crypto::arkworks::serialization::ark_se;
use rand::thread_rng;
use crate::parallel_ark_serde::{par_ark_se_vec, par_ark_de_vec, par_ark_se_vec_vec, par_ark_de_vec_vec};
use rand_core::CryptoRngCore;
use rayon::iter::{IndexedParallelIterator as _, IntoParallelIterator, IntoParallelRefIterator, ParallelIterator as _};
use serde::{Deserialize, Serialize};

use crate::bls_sok::BLSSoK;
use crate::contribution::ContributionInner;
use crate::errors::{BatchSizeNotPowerOfTwo, ContributionVerificationFailure};
use crate::multipairing_equation::{MultipairingEquation, MultipairingEquations};
use crate::powers_of_tau::{PowersOfTauContributionInner, PowersOfTauParams};

type M2C = WBMap<<G1Projective as CurveGroup>::Config>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FPTXContributionInner {
    pub tau_powers_contrib_inner: PowersOfTauContributionInner<Pairing, M2C>,
    #[serde(serialize_with = "par_ark_se_vec", deserialize_with = "par_ark_de_vec")]
    pub alphas_g2: Vec<G2Affine>,
    pub soks_alphas: Vec<BLSSoK<Pairing, M2C>>,
    #[serde(serialize_with = "par_ark_se_vec_vec", deserialize_with = "par_ark_de_vec_vec")]
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
        .into_par_iter()
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
pub struct HashPreimage
where
{
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    previous_alpha_g2: G2Affine,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    alpha_g2: G2Affine,
    index: usize
}


impl ContributionInner for FPTXContributionInner {
    type P = aptos_batch_encryption::group::Pairing;
    type Params = FPTXParams;
    /// No secret for now b/c we don't need this to be composable
    type Secrets = ();
    type Output = DigestKey;

    fn first_contribution(params: &Self::Params) -> Self {
        let trivial_random_alphas_fr = vec![Fr::one(); params.num_rounds];

        let trivial_random_alphas_g2 : Vec<G2Affine> = trivial_random_alphas_fr.par_iter()
            .map(|alpha| G2Affine::from(G2Affine::generator() * alpha))
            .collect();

        let tau_powers_trivial_randomness_fr = tau_powers_randomized_fr(
            params, 
            &vec![Fr::one(); params.batch_size + 1], 
            &trivial_random_alphas_fr
        );

        let tau_powers_trivial_randomness_g1: Vec<Vec<G1Affine>> = tau_powers_trivial_randomness_fr
            .into_par_iter()
            .map(|powers_for_r| G1Projective::from(G1Affine::generator()).batch_mul(&powers_for_r))
            .collect();

        let first_pot_inner = PowersOfTauContributionInner::first_contribution(&PowersOfTauParams { max_power: params.batch_size });

        let sok = BLSSoK::sign(Fr::one(), &String::from(""));

        Self { 
            tau_powers_contrib_inner: first_pot_inner,
            soks_alphas: vec![ sok; trivial_random_alphas_g2.len()],
            alphas_g2: trivial_random_alphas_g2,
            randomized_tau_powers_g1: tau_powers_trivial_randomness_g1,
        }
    }

    fn generate<R: rand_core::CryptoRngCore>(rng: &mut R, previous: &Self, params: &Self::Params) -> (Self, ()) {
        let mut random_alphas_fr = Vec::new();
        for _ in 0..params.num_rounds {
            random_alphas_fr.push(Fr::rand(rng));
        }

        let time = std::time::Instant::now();
        let random_alphas_g2 : Vec<G2Affine> = random_alphas_fr.par_iter()
            .zip(&previous.alphas_g2)
            .map(|(alpha_fr, old_alpha_g2)| G2Affine::from(*old_alpha_g2 * alpha_fr))
            .collect();
        println!("a: {:?}", time.elapsed());

        let time = std::time::Instant::now();

        let soks_alphas : Vec<BLSSoK<Pairing, M2C>> = random_alphas_fr.par_iter().enumerate()
            .zip(&random_alphas_g2)
            .map(|((i, alpha_fr), alpha_g2)| { 
                BLSSoK::sign(*alpha_fr, &HashPreimage { previous_alpha_g2: previous.alphas_g2[i], alpha_g2: *alpha_g2, index: i })

            })
            .collect();
        println!("b: {:?}", time.elapsed());


        let time = std::time::Instant::now();
        let (tau_powers_contrib_inner, tau_powers_fr) = PowersOfTauContributionInner::generate(
            rng, 
            &previous.tau_powers_contrib_inner, 
            &PowersOfTauParams { max_power: params.batch_size }
        );
        println!("c: {:?}", time.elapsed());

        let time = std::time::Instant::now();
        let randomized_tau_powers_fr = tau_powers_randomized_fr(
            params, 
            &tau_powers_fr,
            &random_alphas_fr
        );
        println!("d: {:?}", time.elapsed());

        let time = std::time::Instant::now();
        let randomized_tau_powers_g1p: Vec<Vec<G1Projective>> = randomized_tau_powers_fr
            .into_par_iter()
            .zip(&previous.randomized_tau_powers_g1)
            .map(|(new_scalars_fr, old_g1s)| 
                new_scalars_fr.par_iter()
                    .zip(old_g1s)
                    .map(|(new_scalar, old_g1)| *old_g1 * new_scalar)
                    .collect())
            .collect();
        println!("e: {:?}", time.elapsed());

        // TODO could do batch normalization here, although it doesn't seem to be a significant
        // component of the total time to contribute
        let time = std::time::Instant::now();
        let randomized_tau_powers_g1 : Vec<Vec<G1Affine>> = randomized_tau_powers_g1p
            .into_par_iter()
            .map(|powers_for_round|
                powers_for_round.into_par_iter()
                    .map(G1Affine::from)
                    .collect()
            )
            .collect();
        println!("f: {:?}", time.elapsed());

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
            self.randomized_tau_powers_g1.len() != params.num_rounds {
            return Err(ContributionVerificationFailure::ParamsMismatch)
        }

        let time = std::time::Instant::now();
        let pot_equation = self.tau_powers_contrib_inner.verify(rng, &previous.tau_powers_contrib_inner, &PowersOfTauParams { max_power: params.batch_size })?;
        println!("a {:?}", time.elapsed());
        let time = std::time::Instant::now();


        let sok_check_equation_combined = self.alphas_g2.par_iter().enumerate()
            .zip(&previous.alphas_g2)
            .zip(&self.soks_alphas)
            .map(|(((i, alpha_g2), previous_alpha_g2), sok)| { 
                sok.verify(
                    G2Affine::from(*previous_alpha_g2), 
                    G2Affine::from(*alpha_g2), 
                    &HashPreimage {
                        previous_alpha_g2: previous.alphas_g2[i],
                        alpha_g2: *alpha_g2,
                        index: i,
                    })
            }).collect::<Vec<MultipairingEquation<Pairing>>>()
            .into_iter()
            .fold(MultipairingEquations::new(), |eqs, eq2| eqs.add(eq2));
        

        println!("b {:?}", time.elapsed());

        let time = std::time::Instant::now();

        let alpha_check_equation_combined  = self.alphas_g2.par_iter()
            .zip(&self.randomized_tau_powers_g1)
            .map_init(
                || thread_rng(),
                |rng, (alpha_g2, tau_powers)| 
                MultipairingEquation::with_shared_g2s(
                    rng, 
                    tau_powers.par_iter().zip(&self.tau_powers_contrib_inner.powers)
                        .map(|(randomized_power, nonrandomized_power)|
                            vec![*randomized_power, *nonrandomized_power])
                        .collect(),
                    vec![G2Affine::generator(), -G2Affine::from(*alpha_g2)],
                )
            )
            .collect::<Vec<MultipairingEquation<Pairing>>>()
            .into_iter()
            .fold(MultipairingEquations::new(), |eqs, eq2| eqs.add(eq2));


        println!("c {:?}", time.elapsed());

        Ok(
            MultipairingEquations::new()
                .add(pot_equation)
                .add_eqs(sok_check_equation_combined)
                .add_eqs(alpha_check_equation_combined)
                .compact(rng)
        )

    }

    fn output(&self) -> Self::Output {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use rand::thread_rng;

    use crate::{contribution::ContributionInner, fptx::{FPTXContributionInner, FPTXParams}};


    #[test]
    fn test_ftpx_contribute() {
        let mut rng = thread_rng();
        let params = FPTXParams::new(8, 4).unwrap();

        let first_contrib = FPTXContributionInner::first_contribution(&params);
        let (new_contrib, _) = FPTXContributionInner::generate(&mut rng, &first_contrib, &params);

        new_contrib.verify(&mut rng, &first_contrib, &params)
            .unwrap()
            .equals_zero()
            .unwrap();
    }

    #[test]
    fn test_ftpx_contribute_2() {
        let mut rng = thread_rng();
        let params = FPTXParams::new(1, 1).unwrap();

        let first_contrib = FPTXContributionInner::first_contribution(&params);
        let (new_contrib, _) = FPTXContributionInner::generate(&mut rng, &first_contrib, &params);

        new_contrib.verify(&mut rng, &first_contrib, &params)
            .unwrap()
            .equals_zero()
            .unwrap();

        let (new_contrib_2, _) = FPTXContributionInner::generate(&mut rng, &new_contrib, &params);

        new_contrib_2.verify(&mut rng, &new_contrib, &params)
            .unwrap()
            .equals_zero()
            .unwrap();
    }

    // TODO test invalid contributions
}

