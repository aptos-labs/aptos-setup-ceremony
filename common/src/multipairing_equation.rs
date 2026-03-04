use ark_ec::pairing::{Pairing, PairingOutput};
use ark_ff::Zero;
use ark_std::UniformRand;
use rand_core::CryptoRngCore;

use crate::errors::MultipairingEquationNonZeroResult;

pub struct MultipairingEquation<P>
where
    P: Pairing,
{
    g1s: Vec<P::G1>,
    g2s: Vec<P::G2>,
}

impl<P> MultipairingEquation<P>
where
    P: Pairing,
{
    /// Construct a new multipairing equation. Takes vecs instead of slices so that we can move w/o
    /// calling `to_vec()`, which I believe does a copy.
    pub fn new(g1s: Vec<P::G1>, g2s: Vec<P::G2>) -> Self {
        Self { g1s, g2s }
    }

    fn scalar_mul(self, scalar: P::ScalarField) -> Self {
        Self {
            g1s: self.g1s.into_iter().map( |g1| g1 * scalar ).collect(),
            // I believe the move and the use of `self` instead of `&self` above means no copy here
            g2s: self.g2s
        }
    }

    /// Combine two multipairing equations, resulting in a new equation which equals zero iff
    /// the two input equations equal zero.
    pub fn combine(self, rng: &mut impl CryptoRngCore, other: Self) -> Self {
        let other_scaled = other.scalar_mul(P::ScalarField::rand(rng));
        Self {
            g1s: [self.g1s, other_scaled.g1s].concat(),
            g2s: [self.g2s, other_scaled.g2s].concat(),
        }
    }

    /// Test if this multipairing equation equals zero.
    pub fn equals_zero(&self) -> Result<(), MultipairingEquationNonZeroResult> {
        if P::multi_pairing(&self.g1s, &self.g2s) == PairingOutput::zero() {
            Ok(())
        } else {
            Err(MultipairingEquationNonZeroResult)
        }
    }
}

