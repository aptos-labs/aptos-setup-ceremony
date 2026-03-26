
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
            i.is_multiple_of(2)
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

    

    random_alphas
        .into_par_iter()
        .map(|alpha| {
            tau_powers_fr
                .iter()
                .map(|tau_power| alpha * tau_power)
                .collect::<Vec<Fr>>()
        })
        .collect::<Vec<Vec<Fr>>>()
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
    /// No secret for now b/c we don't need to build other ContributionInners on top of this
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
        let random_alphas_fr : Vec<Fr> = (0..params.num_rounds).map(|_| Fr::rand(rng)).collect();

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


        let sok_check_equations = self.alphas_g2.par_iter().enumerate()
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

        let alpha_check_equations  = self.alphas_g2.par_iter()
            .zip(&self.randomized_tau_powers_g1)
            .map_init(
                thread_rng,
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
                .add_eqs(sok_check_equations)
                .add_eqs(alpha_check_equations)
                .compact(rng)
        )

    }

    fn output(self) -> Self::Output {
        DigestKey::with_randomized_powers_of_tau(
            self.randomized_tau_powers_g1,
            self.tau_powers_contrib_inner.tau_g2,
        )
    }
}

#[cfg(test)]
mod tests {
    use aptos_batch_encryption::{
        schemes::fptx_weighted::FPTXWeighted, tests::smoke::run_smoke, traits::BatchThresholdEncryption as _,
        group::{G1Affine, G2Affine},
    };
    use aptos_crypto::weighted_config::WeightedConfigArkworks;
    use ark_ec::AffineRepr;
    use rand::{Rng as _, thread_rng};
    use crate::{contribution::ContributionInner, fptx::{FPTXContributionInner, FPTXParams}};


    #[test]
    fn test_fptx_contribute() {
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
    fn test_fptx_contribute_2() {
        let mut rng = thread_rng();
        let params = FPTXParams::new(8, 4).unwrap();

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

    #[test]
    #[should_panic]
    fn test_fptx_contribute_invalid() {
        let mut rng = thread_rng();
        let params = FPTXParams::new(8, 4).unwrap();

        let first_contrib = FPTXContributionInner::first_contribution(&params);
        let (mut new_contrib, _) = FPTXContributionInner::generate(&mut rng, &first_contrib, &params);

        new_contrib.soks_alphas[0].sig = G1Affine::from(new_contrib.soks_alphas[0].sig + G1Affine::generator());

        new_contrib.verify(&mut rng, &first_contrib, &params)
            .unwrap()
            .equals_zero()
            .unwrap();
    }

    #[test]
    #[should_panic]
    fn test_fptx_contribute_invalid_2() {
        let mut rng = thread_rng();
        let params = FPTXParams::new(8, 4).unwrap();

        let first_contrib = FPTXContributionInner::first_contribution(&params);
        let (mut new_contrib, _) = FPTXContributionInner::generate(&mut rng, &first_contrib, &params);

        new_contrib.alphas_g2[0] = G2Affine::from(new_contrib.alphas_g2[0] + G2Affine::generator());

        new_contrib.verify(&mut rng, &first_contrib, &params)
            .unwrap()
            .equals_zero()
            .unwrap();
    }

    #[test]
    #[should_panic]
    fn test_fptx_contribute_invalid_3() {
        let mut rng = thread_rng();
        let params = FPTXParams::new(8, 4).unwrap();

        let first_contrib = FPTXContributionInner::first_contribution(&params);
        let (new_contrib, _) = FPTXContributionInner::generate(&mut rng, &first_contrib, &params);

        for i in 0..new_contrib.randomized_tau_powers_g1.len() {
            let mut new_contrib_i = new_contrib.clone();
            new_contrib_i.randomized_tau_powers_g1[i][0] = G1Affine::from(new_contrib_i.randomized_tau_powers_g1[i][0] + G1Affine::generator());

            new_contrib_i.verify(&mut rng, &first_contrib, &params)
                .unwrap()
                .equals_zero()
                .unwrap();
        }

    }

    #[test]
    #[should_panic]
    fn test_fptx_contribute_invalid_4() {
        let mut rng = thread_rng();
        let params = FPTXParams::new(8, 4).unwrap();

        let first_contrib = FPTXContributionInner::first_contribution(&params);
        let (mut new_contrib, _) = FPTXContributionInner::generate(&mut rng, &first_contrib, &params);

        new_contrib.tau_powers_contrib_inner.tau_g2 = G2Affine::from(new_contrib.tau_powers_contrib_inner.tau_g2 + G2Affine::generator());

        new_contrib.verify(&mut rng, &first_contrib, &params)
            .unwrap()
            .equals_zero()
            .unwrap();
    }

    #[test]
    #[should_panic]
    fn test_fptx_contribute_invalid_5() {
        let mut rng = thread_rng();
        let params = FPTXParams::new(8, 4).unwrap();

        let first_contrib = FPTXContributionInner::first_contribution(&params);
        let (mut new_contrib, _) = FPTXContributionInner::generate(&mut rng, &first_contrib, &params);

        new_contrib.tau_powers_contrib_inner.powers[0] = G1Affine::from(new_contrib.tau_powers_contrib_inner.powers[0] + G1Affine::generator());

        new_contrib.verify(&mut rng, &first_contrib, &params)
            .unwrap()
            .equals_zero()
            .unwrap();
    }

    #[test]
    #[should_panic]
    fn test_fptx_contribute_invalid_6() {
        let mut rng = thread_rng();
        let params = FPTXParams::new(8, 4).unwrap();

        let first_contrib = FPTXContributionInner::first_contribution(&params);
        let (mut new_contrib, _) = FPTXContributionInner::generate(&mut rng, &first_contrib, &params);

        new_contrib.tau_powers_contrib_inner.sok.sig = G1Affine::from(new_contrib.tau_powers_contrib_inner.sok.sig + G1Affine::generator());

        new_contrib.verify(&mut rng, &first_contrib, &params)
            .unwrap()
            .equals_zero()
            .unwrap();
    }


    #[test]
    fn test_fptx_output_smoke() {
        let mut rng = thread_rng();
        let params = FPTXParams::new(8, 4).unwrap();

        let first_contrib = FPTXContributionInner::first_contribution(&params);
        let (new_contrib, _) = FPTXContributionInner::generate(&mut rng, &first_contrib, &params);

        new_contrib.verify(&mut rng, &first_contrib, &params)
            .unwrap()
            .equals_zero()
            .unwrap();

        let mut rng = thread_rng();
        let tc = WeightedConfigArkworks::new(3, vec![1, 2, 5]).unwrap();

        let (mut ek, _, vks, msk_shares) =
        FPTXWeighted::setup_for_testing(rng.r#gen(), 8, 1, &tc).unwrap();

        let dk = new_contrib.output();
        ek.use_digest_key(&dk);

        run_smoke::<FPTXWeighted>(tc, ek, dk, vks, msk_shares);
    }

    // TODO test invalid contributions
}

