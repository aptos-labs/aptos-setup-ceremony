use std::marker::PhantomData;

use ark_ec::{
    CurveGroup, PrimeGroup, hashing::{
        HashToCurve, map_to_curve_hasher::{MapToCurve, MapToCurveBasedHasher}
    }, pairing::{Pairing, PairingOutput}
};
use ark_ff::{Zero as _, field_hashers::DefaultFieldHasher};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use aptos_crypto::arkworks::serialization::{ark_de, ark_se};

use crate::{errors::SoKVerificationError, multipairing_equation::MultipairingEquation};




/// A modified BLS SoK which allows you to choose the base point
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BLSSoK<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G1>,
    P::G2: CurveGroup,
    P::G1: CurveGroup,
{
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    sig: P::G1,
    _phantom: PhantomData<fn() -> M2C>,
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
        
        MultipairingEquation::new(vec![P::G1::from(hash_point), self.sig], vec![verification_key, - base_point], )
    }
}


// TODO tests
