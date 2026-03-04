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
#[derive(Serialize, Deserialize)]
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

impl<P: Eq, M2C> Eq for BLSSoK<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G1>,
    P::G2: CurveGroup,
    P::G1: CurveGroup,
{
}

impl<P: PartialEq, M2C> PartialEq for BLSSoK<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G1>,
    P::G2: CurveGroup,
    P::G1: CurveGroup,
{
    fn eq(&self, other: &Self) -> bool {
        self.sig == other.sig && self._phantom == other._phantom
    }
}

impl<P: Clone, M2C> Clone for BLSSoK<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G1>,
    P::G2: CurveGroup,
    P::G1: CurveGroup,
{
    fn clone(&self) -> Self {
        Self { sig: self.sig.clone(), _phantom: self._phantom.clone() }
    }
}

impl<P: std::fmt::Debug, M2C> std::fmt::Debug for BLSSoK<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G1>,
    P::G2: CurveGroup,
    P::G1: CurveGroup,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BLSSoK").field("sig", &self.sig).field("_phantom", &self._phantom).finish()
    }
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
