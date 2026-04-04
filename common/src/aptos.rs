use crate::{contribution::ContributionInner, errors::BatchSizeNotPowerOfTwo, fptx::{FPTXContributionInner, FPTXParams, M2C}, hiding_kzg::{HidingKZGContributionInner, HidingKZGParams}, multipairing_equation::MultipairingEquations};
use aptos_batch_encryption::{group::{Pairing, Fr}, shared::digest::DigestKey};
use aptos_dkg::{pcs::univariate_hiding_kzg, pvss::chunky::chunked_elgamal::num_chunks_per_scalar};
use serde::{Deserialize, Serialize};



/// The inner contribution for our setup ceremony. Consists both
/// of an FPTX inner contribution and a hiding KZG inner contribution,
/// for DeKART.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AptosContributionInner {
    pub fptx: FPTXContributionInner,
    pub hiding_kzg: HidingKZGContributionInner<Pairing, M2C>,
}


#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AptosParams {
    pub fptx: FPTXParams,
    pub hiding_kzg: HidingKZGParams,
}

impl AptosParams {
    pub fn new(
        batch_size: usize, 
        num_rounds: usize,
        max_total_share_weight: usize,
    ) -> Result<Self, BatchSizeNotPowerOfTwo> {

        let num_chunks = num_chunks_per_scalar::<Fr>(aptos_dkg::pvss::chunky::DEFAULT_ELL_FOR_DEPLOYMENT);
        let max_num_chunks_padded = (max_total_share_weight * num_chunks + 1).next_power_of_two() - 1;

        Ok(Self {
            fptx: FPTXParams::new(batch_size, num_rounds)?,
            hiding_kzg: HidingKZGParams::new(max_num_chunks_padded)
        })
    }
}

impl ContributionInner for AptosContributionInner {
    type P = aptos_batch_encryption::group::Pairing;
    type Params = AptosParams;
    type Secrets = ();
    type Output = (DigestKey, (univariate_hiding_kzg::VerificationKey<Pairing>, univariate_hiding_kzg::CommitmentKey<Pairing>));

    fn first_contribution(params: &Self::Params) -> Self {
        Self { 
            fptx: FPTXContributionInner::first_contribution(&params.fptx),
            hiding_kzg: HidingKZGContributionInner::first_contribution(&params.hiding_kzg)
        }
    }

    fn generate<R: rand_core::CryptoRngCore>(rng: &mut R, previous: &Self, params: &Self::Params) -> Result<(Self, Self::Secrets), crate::errors::ContributionGenerationFailure> {
        let fptx = FPTXContributionInner::generate(rng, &previous.fptx, &params.fptx)?.0;
        let hiding_kzg = HidingKZGContributionInner::generate(rng, &previous.hiding_kzg, &params.hiding_kzg)?.0;
        Ok((Self { fptx, hiding_kzg }, ()))
    }

    fn verify(&self, rng: &mut impl rand_core::CryptoRngCore, previous: &Self, params: &Self::Params) -> Result<crate::multipairing_equation::MultipairingEquation<Self::P>, crate::errors::ContributionVerificationFailure> {
        Ok(
            MultipairingEquations::new()
                .add(self.fptx.verify(rng, &previous.fptx, &params.fptx)?)
                .add(self.hiding_kzg.verify(rng, &previous.hiding_kzg, &params.hiding_kzg)?)
                .compact(rng)
        )
    }

    fn output(self) -> Self::Output {
        (self.fptx.output(), self.hiding_kzg.output())
    }
}

#[cfg(test)]
mod tests {
    use aptos_batch_encryption::{schemes::fptx_weighted::FPTXWeighted, tests::smoke::{fptx_weighted_smoke::run_pvss_with_hkzg, run_smoke}};
    use rand::thread_rng;

    use crate::{aptos::{AptosContributionInner, AptosParams}, contribution::ContributionInner as _};


    #[test]
    fn test_aptos_output_smoke() {
        let mut rng = thread_rng();
        let params = AptosParams::new(8, 4, 8).unwrap();

        let first_contrib : AptosContributionInner = AptosContributionInner::first_contribution(&params);
        let (new_contrib, _) = AptosContributionInner::generate(&mut rng, &first_contrib, &params).unwrap();

        new_contrib.verify(&mut rng, &first_contrib, &params)
            .unwrap()
            .equals_zero()
            .unwrap();


        let (dk, hkzg_setup) = new_contrib.output();


        let (tc, ek, vks, msk_shares) = run_pvss_with_hkzg(&dk, (hkzg_setup.1, hkzg_setup.0));


        run_smoke::<FPTXWeighted>(tc, ek, dk, vks, msk_shares);
    }
}
