//! Compute wire sizes without allocating the full CRS or discrete-log table.
use aptos_batch_encryption::{
    group::{G1Affine, G1Projective, G2Affine},
    shared::digest_key_file::Header,
};
use aptos_dkg::{
    pvss::chunky::DEFAULT_ELL_FOR_DEPLOYMENT,
    range_proofs::{dekart_univariate_v2, traits::BatchedRangeProof},
};
use ark_ec::AffineRepr;
use ark_serialize::{CanonicalSerialize, Compress};
use common::{aptos::{AptosContributionInner, AptosParams}, constants::PARAMS, contribution::ContributionInner};

fn uleb128_len(mut value: usize) -> usize {
    let mut len = 1;
    while value >= 128 {
        value >>= 7;
        len += 1;
    }
    len
}

fn bcs_bytes_size(len: usize) -> usize {
    uleb128_len(len) + len
}

fn contribution_core_size(params: &AptosParams) -> usize {
    let g1 = G1Affine::generator().compressed_size();
    let g2 = G2Affine::generator().compressed_size();
    let powers = params.fptx.batch_size + 1;
    let rounds = params.fptx.num_rounds;
    // Arkworks vectors have an 8-byte element count, inside a BCS byte vector.
    let pot = |n| bcs_bytes_size(8 + n * g1) + bcs_bytes_size(g2) + bcs_bytes_size(g1);
    let fptx = pot(powers)
        + bcs_bytes_size(8 + rounds * g2)
        + uleb128_len(rounds) + rounds * bcs_bytes_size(g1)
        + bcs_bytes_size(8 + rounds * (8 + powers * g1));
    let hkzg = pot(params.hiding_kzg.max_power + 1)
        + bcs_bytes_size(g1) + bcs_bytes_size(g2) + bcs_bytes_size(g1);
    fptx + hkzg
}

fn digest_key_size(batch_size: usize, num_rounds: usize) -> usize {
    Header::representation_size_bytes()
        + num_rounds * ((batch_size + 1) * G1Affine::generator().uncompressed_size()
            + 2 * batch_size * G1Projective::from(G1Affine::generator()).uncompressed_size())
}

fn pp_size(prover_key_bytes: usize, table_entries: usize) -> usize {
    let g1 = G1Affine::generator().compressed_size();
    let g2 = G2Affine::generator().compressed_size();
    // Two ElGamal bases, the range-proof key, G2, and three usize fields.
    let public_params = 2 * bcs_bytes_size(g1) + bcs_bytes_size(prover_key_bytes)
        + bcs_bytes_size(g2) + 3 * 8;
    // BabyStepTable: G, map<compressed point, u32>, table_size, -table_size*G.
    let table = 2 * bcs_bytes_size(g1) + 8 + uleb128_len(table_entries)
        + table_entries * (bcs_bytes_size(g1) + 4);
    public_params + table
}

fn print_size(label: &str, bytes: usize) {
    println!("{label}: {bytes} bytes ({:.6} GB; {:.6} GiB)",
        bytes as f64 / 1e9, bytes as f64 / 1024f64.powi(3));
}

fn main() {
    let params = &*PARAMS;
    let batch = params.fptx.batch_size;
    let rounds = params.fptx.num_rounds;
    println!("BN254: batch size {batch}, rounds {rounds}, hiding-KZG powers {}",
        params.hiding_kzg.max_power + 1);
    println!("Compressed G1/G2: {}/{} bytes; uncompressed G1: {} bytes",
        G1Affine::generator().compressed_size(), G2Affine::generator().compressed_size(),
        G1Affine::generator().uncompressed_size());
    print_size("BTE CRS points, compressed (no cache/framing)",
        rounds * (batch + 1) * G1Affine::generator().compressed_size()
            + G2Affine::generator().compressed_size());
    print_size("Ceremony contribution core (BCS; excludes contributor/history)",
        contribution_core_size(params));
    print_size("digest_key.bin (includes uncompressed FFT cache)", digest_key_size(batch, rounds));

    // Only build one round and the small hiding-KZG component. Dummy points
    // suffice for size accounting; no production setup is generated here.
    let mut small = params.clone();
    small.fptx.num_rounds = 1;
    let (_, (vk, ck)) = AptosContributionInner::first_contribution(&small).output();
    let (pk, _) = dekart_univariate_v2::Proof::setup(DEFAULT_ELL_FOR_DEPLOYMENT, vk, ck);
    // Match the 256-player config in client/src/run.rs and the extra 4 bits
    // in chunky::PublicParameters::build_dlog_table.
    let max_aggregation = 256;
    let table_entries = 1usize << (4 + (DEFAULT_ELL_FOR_DEPLOYMENT + ark_std::log2(max_aggregation) as usize) / 2);
    println!("pp.bin assumes max aggregation {max_aggregation}; dlog table entries {table_entries}");
    let pp = pp_size(pk.serialized_size(Compress::Yes), table_entries);
    print_size("pp.bin (includes the discrete-log table)", pp);
    print_size("Combined exported files", digest_key_size(batch, rounds) + pp);
}

#[cfg(test)]
mod tests {
    use super::*;
    use aptos_batch_encryption::group::Pairing;
    use aptos_batch_encryption::shared::digest_key_file::{read_digest_key, write_digest_key};
    use aptos_dkg::pvss::chunky::PublicParameters;

    #[test]
    fn contribution_estimate_matches_serialization() {
        for (batch, rounds, weight) in [(1, 1, 1), (8, 2, 8), (128, 129, 256)] {
            let params = AptosParams::new(batch, rounds, weight).unwrap();
            let contribution = AptosContributionInner::first_contribution(&params);
            assert_eq!(contribution_core_size(&params), bcs::to_bytes(&contribution).unwrap().len());
        }
    }

    #[test]
    fn digest_estimate_matches_exported_file() {
        let params = AptosParams::new(8, 3, 8).unwrap();
        let (dk, _) = AptosContributionInner::first_contribution(&params).output();
        let expected = dk.clone();
        let file = tempfile::NamedTempFile::new().unwrap();
        write_digest_key(file.path(), dk).unwrap();
        assert_eq!(digest_key_size(8, 3), file.as_file().metadata().unwrap().len() as usize);
        assert_eq!(expected, read_digest_key(file.path()).unwrap());
    }

    #[test]
    fn pp_estimate_matches_serialization() {
        // The testing defaults use a small table; the same framing applies
        // to the production-sized table without allocating millions of points.
        let pp = PublicParameters::<Pairing>::default();
        assert_eq!(pp_size(pp.pk_range_proof.compressed_size(), pp.dlog_table.table_size),
            bcs::to_bytes(&pp).unwrap().len());
    }
}
