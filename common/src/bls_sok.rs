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

use crate::errors::SoKVerificationError;




#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BLSSoK<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G2>,
    P::G2: CurveGroup,
    P::G1: CurveGroup,
{
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    sig: P::G2,
    _phantom: PhantomData<M2C>,
}


impl<P, M2C> BLSSoK<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G2>,
    P::G2: CurveGroup,
    P::G1: CurveGroup,
{
    pub fn base_point(msg: &impl Serialize) -> P::G2Affine {
        let hasher: MapToCurveBasedHasher<P::G2, DefaultFieldHasher<Sha256>, M2C> = MapToCurveBasedHasher::new(&[0u8]).unwrap();

        hasher.hash(
            &bcs::to_bytes(&msg)
                .expect("BCS should never fail to serialize (see failure conditions in bcs docs)")
        ).unwrap()
    }

    pub fn sign(
        secret_scalar: P::ScalarField,
        msg: &impl Serialize,
    ) -> Self {


        let base_point = Self::base_point(msg);

        Self {
            sig: base_point * secret_scalar,
            _phantom: PhantomData
        }
    }

    /// TODO batch verification?
    pub fn verify(
        &self,
        verification_key: P::G1,
        msg: &impl Serialize,
    ) -> Result<(), SoKVerificationError> {
        let base_point = Self::base_point(msg);
        
        if P::multi_pairing([verification_key, -<P::G1 as PrimeGroup>::generator()], [P::G2::from(base_point), self.sig]) != PairingOutput::zero() {
            Ok(())
        } else {
            Err(SoKVerificationError)
        }

    }
}
