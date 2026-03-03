use ark_ec::{CurveGroup, VariableBaseMSM};
use ark_ff::field_hashers::{DefaultFieldHasher, HashToField};
use ark_std::UniformRand;
use rand_core::CryptoRngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use aptos_crypto::arkworks::serialization::{ark_se, ark_de};
use crate::errors::SoKVerificationError;




#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatchedSigOfKnowledge<G: CurveGroup + VariableBaseMSM> {
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    round_1: G,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    round_3: G::ScalarField,
}

/// Represents the data that is hashed during the sigma protocol for the Fiat-Shamir challenge.
/// Using a struct for this so that we don't have to worry about domain-separating each element
/// manually.
#[derive(Serialize)]
struct HashPreimage<G: CurveGroup, M: Serialize> {
    #[serde(serialize_with = "ark_se")]
    target_elts: Vec<G::Affine>,
    #[serde(serialize_with = "ark_se")]
    round_1: G,
    msg: M,
}

impl<G: CurveGroup + VariableBaseMSM> BatchedSigOfKnowledge<G> {
    fn compute_hash_challenge<M: Serialize + Clone>(
        target_elts: &[G::Affine],
        round_1: G,
        msg: &M,
    ) -> G::ScalarField {
        let preimage = HashPreimage {
            target_elts: Vec::from(target_elts),
            round_1,
            msg: msg.clone(),
        };

        let field_hasher = <DefaultFieldHasher<Sha256> as HashToField<G::ScalarField>>::new(&[]);
        field_hasher.hash_to_field::<1>(
            &bcs::to_bytes(&preimage)
            .expect("BCS should never fail to serialize (see failure conditions in bcs docs)")
        )[0]
    }

    pub fn sign(
        rng: &mut impl CryptoRngCore,
        target_elts: &[G::Affine],
        secret_exponents: &[G::ScalarField],
        msg: &impl Serialize,
    ) -> Self {
        assert_eq!(target_elts.len(), secret_exponents.len());

        let round_1_scalar = G::ScalarField::rand(rng);
        let round_1 = G::generator() * round_1_scalar;

        let round_2_challenge = Self::compute_hash_challenge(target_elts, round_1, &msg);

        let round_3 : G::ScalarField = round_1_scalar + 
            std::iter::successors(Some(round_2_challenge), |p| Some(*p * round_2_challenge))
            .take(secret_exponents.len())
            .zip(secret_exponents)
            .map(|(challenge_power, secret_exponent)| challenge_power * secret_exponent)
            .sum::<G::ScalarField>();

        Self { round_1, round_3 }
    }

    pub fn verify(
        &self,
        target_elts: &[G::Affine],
        msg: &impl Serialize,
    ) -> Result<(), SoKVerificationError> {

        let round_2_challenge = Self::compute_hash_challenge(target_elts, self.round_1, &msg);

        let round_2_challenge_powers : Vec<G::ScalarField> =
        std::iter::successors(Some(round_2_challenge), |p| Some(*p * round_2_challenge))
            .take(target_elts.len())
            .collect();

        if G::generator() * self.round_3 == self.round_1 + G::msm_unchecked(target_elts, &round_2_challenge_powers) {
            Ok(())
        } else {
            Err(SoKVerificationError)
        }
    }
        
}

#[cfg(test)]
mod tests {
    use aptos_batch_encryption::group::{Fr, G2Affine, G2Projective};
    use ark_ec::AffineRepr;
    use ark_ff::UniformRand;
    use ark_std::rand::thread_rng;

    use crate::batched_schnorr::BatchedSigOfKnowledge;

    #[test]
    fn test_sign_verify() {
        let mut rng = thread_rng();

        let mut random_secret_scalars = Vec::new();
        for _ in 0..10 {
            random_secret_scalars.push(Fr::rand(&mut rng));
        }

        let target_elts : Vec<G2Affine> = random_secret_scalars
            .iter()
            .map(|scalar| G2Affine::from(G2Affine::generator() * scalar))
            .collect();

        let msg = "hi";

        let sok : BatchedSigOfKnowledge<G2Projective> = BatchedSigOfKnowledge::sign(&mut rng, &target_elts, &random_secret_scalars, &msg);

        sok.verify(&target_elts, &msg).unwrap();
    }
}

