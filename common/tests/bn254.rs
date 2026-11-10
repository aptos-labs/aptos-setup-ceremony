use aptos_batch_encryption::group::{Fr, G1Affine, G1Hasher, G1Projective, G2Affine, Pairing};
use ark_ec::{hashing::HashToCurve, AffineRepr};
use ark_serialize::CanonicalSerialize;
use common::bls_sok::BLSSoK;

#[test]
fn bn254_hash_to_curve_domain_and_subgroup() {
    // These type checks prevent silently switching the artifact back to BLS12-381.
    let _: ark_bn254::G1Affine = G1Affine::generator();
    let _: ark_bn254::G2Affine = G2Affine::generator();
    assert_eq!(G1Affine::generator().compressed_size(), 32);
    assert_eq!(G2Affine::generator().compressed_size(), 64);

    let hasher = G1Hasher::new(b"ceremony-test-domain").unwrap();
    for i in 0u64..64 {
        let msg = i.to_le_bytes();
        let point = hasher.hash(&msg).unwrap();
        assert!(point.is_on_curve());
        assert!(point.is_in_correct_subgroup_assuming_on_curve());
        assert!(!point.is_zero());
        assert_eq!(point, hasher.hash(&msg).unwrap());
        assert_ne!(point, hasher.hash(&(i + 1).to_le_bytes()).unwrap());
        assert_ne!(point, G1Hasher::new(b"other-domain").unwrap().hash(&msg).unwrap());
    }
    assert!(<G1Hasher as HashToCurve<G1Projective>>::new(&[0; 256]).is_err());
}

#[test]
fn bn254_sok_binds_message_and_key() {
    let base = G2Affine::generator();
    let secret = Fr::from(7u64);
    let key = G2Affine::from(base * secret);
    let proof = BLSSoK::<Pairing, G1Hasher>::sign(secret, &"contribution");
    proof.verify(base, key, &"contribution").equals_zero().unwrap();
    assert!(proof.verify(base, key, &"different contribution").equals_zero().is_err());
    let other_key = G2Affine::from(base * Fr::from(8u64));
    assert!(proof.verify(base, other_key, &"contribution").equals_zero().is_err());
}
