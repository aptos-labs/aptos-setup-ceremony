// Copyright (c) Aptos Foundation
// Licensed pursuant to the Innovation-Enabling Source Code License, available at https://github.com/aptos-labs/aptos-core/blob/main/LICENSE
pub use ark_bn254::{
    g1::Config as G1Config, Bn254 as PairingSetting, Config, Fq, Fr, G1Affine, G1Projective,
    G2Affine, G2Projective,
};

pub type G1Prepared = <Pairing as ark_ec::pairing::Pairing>::G1Prepared;
pub type G2Prepared = <Pairing as ark_ec::pairing::Pairing>::G2Prepared;
pub type PairingOutput = ark_ec::pairing::PairingOutput<Pairing>;
pub type Pairing = ark_bn254::Bn254;

/// BN254 hash-to-G1 for public inputs, using the existing SHA3-512
/// try-and-increment implementation. This is variable-time, not the
/// BLS12-381-specific RFC 9380 WB map.
pub struct G1Hasher {
    domain: Vec<u8>,
}

impl ark_ec::hashing::HashToCurve<G1Projective> for G1Hasher {
    fn new(domain: &[u8]) -> Result<Self, ark_ec::hashing::HashToCurveError> {
        if domain.len() > u8::MAX as usize {
            return Err(ark_ec::hashing::HashToCurveError::MapToCurveError(
                "Hash-to-curve domain exceeds 255 bytes".to_string(),
            ));
        }
        Ok(Self { domain: domain.to_vec() })
    }

    fn hash(&self, message: &[u8]) -> Result<G1Affine, ark_ec::hashing::HashToCurveError> {
        Ok(aptos_crypto::arkworks::hashing::unsafe_hash_to_affine(
            message, &self.domain,
        ))
    }
}
