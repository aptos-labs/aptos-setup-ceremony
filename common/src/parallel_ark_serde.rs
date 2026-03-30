use ark_serialize::{CanonicalDeserialize, CanonicalSerialize, Compress, Validate};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use rayon::slice::ParallelSlice;

struct ByteBufVisitor;

impl<'de> serde::de::Visitor<'de> for ByteBufVisitor {
    type Value = Vec<u8>;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("byte buf")
    }

    fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
        Ok(v.to_vec())
    }

    fn visit_byte_buf<E: serde::de::Error>(self, v: Vec<u8>) -> Result<Self::Value, E> {
        Ok(v)
    }
}

fn deserialize_bytes<'de, D: serde::de::Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    d.deserialize_byte_buf(ByteBufVisitor)
}

// --- Vec<T> ---

pub fn par_ark_se_vec<S, T>(vec: &Vec<T>, s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    T: CanonicalSerialize + Sync,
{
    let elem_bytes: Vec<Vec<u8>> = vec
        .par_chunks(1)
        .map(|chunk| {
            let mut buf = Vec::new();
            chunk[0]
                .serialize_with_mode(&mut buf, Compress::Yes)
                .unwrap();
            buf
        })
        .collect();

    let total: usize = 8 + elem_bytes.iter().map(|b| b.len()).sum::<usize>();
    let mut bytes = Vec::with_capacity(total);
    bytes.extend_from_slice(&(vec.len() as u64).to_le_bytes());
    for eb in &elem_bytes {
        bytes.extend_from_slice(eb);
    }

    s.serialize_bytes(&bytes)
}

pub fn par_ark_de_vec<'de, D, T>(data: D) -> Result<Vec<T>, D::Error>
where
    D: serde::de::Deserializer<'de>,
    T: CanonicalDeserialize + Send + Sync,
{
    let raw = deserialize_bytes(data)?;

    if raw.len() < 8 {
        return Err(serde::de::Error::custom("not enough bytes for length prefix"));
    }

    let len = u64::from_le_bytes(raw[0..8].try_into().unwrap()) as usize;

    if len == 0 {
        return Ok(Vec::new());
    }

    let rest = &raw[8..];

    if rest.len() % len != 0 {
        return Err(serde::de::Error::custom(
            "element bytes not evenly divisible by count",
        ));
    }

    let es = rest.len() / len;

    rest.par_chunks(es)
        .map(|chunk| {
            #[cfg(feature = "validate")]
            let result = T::deserialize_with_mode(&mut &*chunk, Compress::Yes, Validate::No)
                .map_err(|e| e.to_string());
            #[cfg(not(feature = "validate"))]
            let result = T::deserialize_with_mode(&mut &*chunk, Compress::Yes, Validate::Yes)
                .map_err(|e| e.to_string());
            result
        })
        .collect::<Result<Vec<T>, String>>()
        .map_err(serde::de::Error::custom)
}

// --- Vec<Vec<T>> ---

pub fn par_ark_se_vec_vec<S, T>(vec: &Vec<Vec<T>>, s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    T: CanonicalSerialize + Sync + Send,
{
    let inner_bytes: Vec<Vec<u8>> = vec
        .into_par_iter()
        .map(|inner| {
            let elem_bytes: Vec<Vec<u8>> = inner
                .par_chunks(1)
                .map(|chunk| {
                    let mut buf = Vec::new();
                    chunk[0]
                        .serialize_with_mode(&mut buf, Compress::Yes)
                        .unwrap();
                    buf
                })
                .collect();

            let total: usize = 8 + elem_bytes.iter().map(|b| b.len()).sum::<usize>();
            let mut buf = Vec::with_capacity(total);
            buf.extend_from_slice(&(inner.len() as u64).to_le_bytes());
            for eb in &elem_bytes {
                buf.extend_from_slice(eb);
            }
            buf
        })
        .collect();

    let total: usize = 8 + inner_bytes.iter().map(|b| b.len()).sum::<usize>();
    let mut bytes = Vec::with_capacity(total);
    bytes.extend_from_slice(&(vec.len() as u64).to_le_bytes());
    for ib in &inner_bytes {
        bytes.extend_from_slice(ib);
    }

    s.serialize_bytes(&bytes)
}

pub fn par_ark_de_vec_vec<'de, D, T>(data: D) -> Result<Vec<Vec<T>>, D::Error>
where
    D: serde::de::Deserializer<'de>,
    T: CanonicalDeserialize + Send + Sync,
{
    let raw = deserialize_bytes(data)?;

    if raw.len() < 8 {
        return Err(serde::de::Error::custom("not enough bytes for length prefix"));
    }

    let outer_len = u64::from_le_bytes(raw[0..8].try_into().unwrap()) as usize;

    if outer_len == 0 {
        return Ok(Vec::new());
    }

    // Compute element size from the first inner vec
    if raw.len() < 16 {
        return Err(serde::de::Error::custom("truncated inner vec length"));
    }
    let first_inner_len = u64::from_le_bytes(raw[8..16].try_into().unwrap()) as usize;
    if first_inner_len == 0 {
        // All inner vecs must be empty; just verify structure
        let expected = 8 + outer_len * 8;
        if raw.len() != expected {
            return Err(serde::de::Error::custom("unexpected trailing bytes"));
        }
        return Ok((0..outer_len).map(|_| Vec::new()).collect());
    }

    // Determine element byte size by trial-deserializing the first element
    let trial_start = &raw[16..];
    let mut cursor = std::io::Cursor::new(trial_start);
    let _trial: T = T::deserialize_with_mode(&mut cursor, Compress::Yes, Validate::No)
        .map_err(|e| serde::de::Error::custom(e.to_string()))?;
    let es = cursor.position() as usize;

    // Now scan all inner vec boundaries
    let mut ranges: Vec<(usize, usize)> = Vec::with_capacity(outer_len);
    let mut pos = 8usize;
    for _ in 0..outer_len {
        if pos + 8 > raw.len() {
            return Err(serde::de::Error::custom("truncated inner vec length"));
        }
        let inner_len = u64::from_le_bytes(raw[pos..pos + 8].try_into().unwrap()) as usize;
        let start = pos + 8;
        let end = start + inner_len * es;
        if end > raw.len() {
            return Err(serde::de::Error::custom("truncated inner vec data"));
        }
        ranges.push((start, inner_len));
        pos = end;
    }

    ranges
        .into_par_iter()
        .map(|(start, inner_len)| {
            let data = &raw[start..start + inner_len * es];
            data.par_chunks(es)
                .map(|chunk| {
                    #[cfg(feature = "validate")]
                    let result = T::deserialize_with_mode(&mut &*chunk, Compress::Yes, Validate::Yes)
                        .map_err(|e| e.to_string());
                    #[cfg(not(feature = "validate"))]
                    let result = T::deserialize_with_mode(&mut &*chunk, Compress::Yes, Validate::No)
                        .map_err(|e| e.to_string());

                    result
                })
                .collect::<Result<Vec<T>, String>>()
        })
        .collect::<Result<Vec<Vec<T>>, String>>()
        .map_err(serde::de::Error::custom)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aptos_batch_encryption::group::{G1Affine, G1Projective, G2Affine};
    use aptos_crypto::arkworks::serialization::{ark_de, ark_se};
    use ark_ec::AffineRepr;
    use serde::{Deserialize, Serialize};

    /// Wrapper using the original sequential ark_se/ark_de for Vec<G2Affine>
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct OrigVecG2 {
        #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
        v: Vec<G2Affine>,
    }

    /// Wrapper using parallel serde for Vec<G2Affine>
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct ParVecG2 {
        #[serde(serialize_with = "par_ark_se_vec", deserialize_with = "par_ark_de_vec")]
        v: Vec<G2Affine>,
    }

    /// Wrapper using the original sequential ark_se/ark_de for Vec<Vec<G1Affine>>
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct OrigVecVecG1 {
        #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
        v: Vec<Vec<G1Affine>>,
    }

    /// Wrapper using parallel serde for Vec<Vec<G1Affine>>
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct ParVecVecG1 {
        #[serde(
            serialize_with = "par_ark_se_vec_vec",
            deserialize_with = "par_ark_de_vec_vec"
        )]
        v: Vec<Vec<G1Affine>>,
    }

    fn sample_g2_vec() -> Vec<G2Affine> {
        let g = G2Affine::generator();
        let mut pts = vec![G2Affine::zero()];
        let mut acc = g.into_group();
        for _ in 0..5 {
            pts.push(acc.into());
            acc += acc;
        }
        pts
    }

    fn sample_g1_vec_vec() -> Vec<Vec<G1Affine>> {
        let g = G1Affine::generator();
        (0..3)
            .map(|shift| {
                let mut pts = Vec::new();
                let mut acc = G1Projective::from(g);
                for _ in 0..shift + 2 {
                    pts.push(acc.into());
                    acc += acc;
                }
                pts
            })
            .collect()
    }

    #[test]
    fn test_vec_wire_format_identical() {
        let v = sample_g2_vec();

        let orig_bytes = bcs::to_bytes(&OrigVecG2 { v: v.clone() }).unwrap();
        let par_bytes = bcs::to_bytes(&ParVecG2 { v: v.clone() }).unwrap();
        assert_eq!(orig_bytes, par_bytes, "serialized bytes must be identical");

        // Cross-deserialize: parallel-serialized bytes deserialized with original
        let from_par: OrigVecG2 = bcs::from_bytes(&par_bytes).unwrap();
        assert_eq!(from_par.v, v);

        // Cross-deserialize: original-serialized bytes deserialized with parallel
        let from_orig: ParVecG2 = bcs::from_bytes(&orig_bytes).unwrap();
        assert_eq!(from_orig.v, v);
    }

    #[test]
    fn test_vec_empty() {
        let v: Vec<G2Affine> = vec![];
        let orig_bytes = bcs::to_bytes(&OrigVecG2 { v: v.clone() }).unwrap();
        let par_bytes = bcs::to_bytes(&ParVecG2 { v: v.clone() }).unwrap();
        assert_eq!(orig_bytes, par_bytes);
        let rt: ParVecG2 = bcs::from_bytes(&par_bytes).unwrap();
        assert_eq!(rt.v, v);
    }

    #[test]
    fn test_vec_vec_wire_format_identical() {
        let v = sample_g1_vec_vec();

        let orig_bytes = bcs::to_bytes(&OrigVecVecG1 { v: v.clone() }).unwrap();
        let par_bytes = bcs::to_bytes(&ParVecVecG1 { v: v.clone() }).unwrap();
        assert_eq!(orig_bytes, par_bytes, "serialized bytes must be identical");

        // Cross-deserialize
        let from_par: OrigVecVecG1 = bcs::from_bytes(&par_bytes).unwrap();
        assert_eq!(from_par.v, v);

        let from_orig: ParVecVecG1 = bcs::from_bytes(&orig_bytes).unwrap();
        assert_eq!(from_orig.v, v);
    }

    #[test]
    fn test_vec_vec_empty_outer() {
        let v: Vec<Vec<G1Affine>> = vec![];
        let orig_bytes = bcs::to_bytes(&OrigVecVecG1 { v: v.clone() }).unwrap();
        let par_bytes = bcs::to_bytes(&ParVecVecG1 { v: v.clone() }).unwrap();
        assert_eq!(orig_bytes, par_bytes);
        let rt: ParVecVecG1 = bcs::from_bytes(&par_bytes).unwrap();
        assert_eq!(rt.v, v);
    }
}
