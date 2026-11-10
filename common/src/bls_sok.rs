use std::{marker::PhantomData, ops::Neg};

use ark_ec::{
    AffineRepr, hashing::HashToCurve, pairing::Pairing
};
use serde::{Deserialize, Serialize};
use aptos_crypto::arkworks::serialization::{ark_de, ark_se};

use crate::multipairing_equation::MultipairingEquation;




/// A modified BLS SoK which allows you to choose the base point
#[derive(Serialize, Deserialize)]
pub struct BLSSoK<P, M2C>
where
    P: Pairing,
    M2C: HashToCurve<P::G1>,
    P::G2Affine: AffineRepr + Neg<Output = P::G2Affine>,
    P::G1Affine: AffineRepr,
{
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    pub(crate) sig: P::G1Affine,
    pub(crate) _phantom: PhantomData<M2C>,
}


impl<P, M2C> BLSSoK<P, M2C>
where
    P: Pairing,
    M2C: HashToCurve<P::G1>,
    P::G2Affine: AffineRepr + Neg<Output = P::G2Affine>,
    P::G1Affine: AffineRepr,
{
    pub fn hash_point(msg: &impl Serialize) -> P::G1Affine {
        let hasher = M2C::new(b"TRX_CEREMONY_BLS_SOK_BN254_V1").unwrap();

        hasher.hash(
            &bcs::to_bytes(&msg)
                .expect("BCS should never fail to serialize (see failure conditions in bcs docs)")
        ).unwrap()
    }

    pub fn sign(
        secret_scalar: P::ScalarField,
        msg: &impl Serialize,
    ) -> Self {
        let hash_point = Self::hash_point(msg);

        Self {
            sig: P::G1Affine::from(hash_point * secret_scalar),
            _phantom: PhantomData
        }
    }

    pub fn verify(
        &self,
        base_point: P::G2Affine,
        verification_key: P::G2Affine,
        msg: &impl Serialize,
    ) -> MultipairingEquation<P> {
        let hash_point = Self::hash_point(msg);
        
        MultipairingEquation::simple(vec![hash_point, self.sig], vec![verification_key, - base_point])
    }
}


#[cfg(test)]
mod tests {
    use aptos_batch_encryption::group::{Fr, G1Hasher, Pairing, G2Affine, G1Affine};
    use ark_ec::AffineRepr;
    use ark_ff::UniformRand;
    use rand::thread_rng;

    use crate::bls_sok::BLSSoK;
    type M2C = G1Hasher;

    #[test]
    fn test_bls_sign_and_verify() {
        let mut rng = thread_rng();
        let base_point = G2Affine::from(G2Affine::generator() * Fr::rand(&mut rng));
        let secret = Fr::rand(&mut rng);
        let verification_key = G2Affine::from(base_point * secret);
        let msg = String::from("hi");

        let sig : BLSSoK<Pairing, M2C> = BLSSoK::sign(secret, &msg);

        sig.verify(base_point, verification_key, &msg).equals_zero().unwrap();
    }

    #[test]
    #[should_panic]
    fn test_bls_sign_and_verify_invalid() {
        let mut rng = thread_rng();
        let base_point = G2Affine::from(G2Affine::generator() * Fr::rand(&mut rng));
        let secret = Fr::rand(&mut rng);
        let verification_key = G2Affine::from(base_point * secret);
        let msg = String::from("hi");

        let mut sig : BLSSoK<Pairing, M2C> = BLSSoK::sign(secret, &msg);
        sig.sig = G1Affine::from(sig.sig + G1Affine::generator());

        sig.verify(base_point, verification_key, &msg).equals_zero().unwrap();
    }
}
