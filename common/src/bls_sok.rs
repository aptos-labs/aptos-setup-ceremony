use std::marker::PhantomData;

use ark_ec::{
    CurveGroup, hashing::{
        HashToCurve, map_to_curve_hasher::{MapToCurve, MapToCurveBasedHasher}
    }, pairing::Pairing
};
use ark_ff::field_hashers::DefaultFieldHasher;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use aptos_crypto::arkworks::serialization::{ark_de, ark_se};

use crate::multipairing_equation::MultipairingEquation;




/// A modified BLS SoK which allows you to choose the base point
#[derive(Serialize, Deserialize)]
pub struct BLSSoK<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G1>,
    P::G2: CurveGroup,
    P::G1: CurveGroup,
{
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    pub(crate) sig: P::G1,
    pub(crate) _phantom: PhantomData<M2C>,
}


impl<P, M2C> BLSSoK<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G1>,
    P::G2: CurveGroup,
    P::G1: CurveGroup,
{
    pub fn hash_point(msg: &impl Serialize) -> P::G1Affine {
        let hasher: MapToCurveBasedHasher<P::G1, DefaultFieldHasher<Sha256>, M2C> = MapToCurveBasedHasher::new(&[0u8]).unwrap();

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
            sig: hash_point * secret_scalar,
            _phantom: PhantomData
        }
    }

    pub fn verify(
        &self,
        base_point: P::G2,
        verification_key: P::G2,
        msg: &impl Serialize,
    ) -> MultipairingEquation<P> {
        let hash_point = Self::hash_point(msg);
        
        MultipairingEquation::simple(vec![P::G1::from(hash_point), self.sig], vec![verification_key, - base_point], )
    }
}


#[cfg(test)]
mod tests {
    use aptos_batch_encryption::group::{Fr, G2Projective, G1Projective, Pairing};
    use ark_ec::{CurveGroup, PrimeGroup, hashing::curve_maps::wb::WBMap};
    use ark_ff::UniformRand;
    use rand::thread_rng;

    use crate::bls_sok::BLSSoK;
    type M2C = WBMap<<G1Projective as CurveGroup>::Config>;

    #[test]
    fn test_bls_sign_and_verify() {
        let mut rng = thread_rng();
        let base_point = G2Projective::generator() * Fr::rand(&mut rng);
        let secret = Fr::rand(&mut rng);
        let verification_key = base_point * secret;
        let msg = String::from("hi");

        let sig : BLSSoK<Pairing, M2C> = BLSSoK::sign(secret, &msg);

        sig.verify(base_point, verification_key, &msg).equals_zero().unwrap();
    }

    #[test]
    #[should_panic]
    fn test_bls_sign_and_verify_invalid() {
        let mut rng = thread_rng();
        let base_point = G2Projective::generator() * Fr::rand(&mut rng);
        let secret = Fr::rand(&mut rng);
        let verification_key = base_point * secret;
        let msg = String::from("hi");

        let mut sig : BLSSoK<Pairing, M2C> = BLSSoK::sign(secret, &msg);
        sig.sig += G1Projective::generator();

        sig.verify(base_point, verification_key, &msg).equals_zero().unwrap();
    }
}
