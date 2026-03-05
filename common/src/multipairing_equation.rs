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

    pub fn empty() -> Self {
        Self::new(vec![], vec![])
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

#[cfg(test)]
mod tests {
    use aptos_batch_encryption::group::{Fr, G1Projective, G2Projective, Pairing};
    use ark_ec::PrimeGroup;
    use ark_std::UniformRand;
    use ark_std::Zero;
    use rand::thread_rng;

    use crate::multipairing_equation::MultipairingEquation;

    fn make_eq() -> MultipairingEquation<Pairing> {
        let mut rng = thread_rng();

        let mut frs_1 = Vec::new();
        for _ in 0..4 {
            frs_1.push(Fr::rand(&mut rng));
        }

        let mut frs_2 = Vec::new();
        for _ in 0..3 {
            frs_2.push(Fr::rand(&mut rng));
        }

        let partial_sum : Fr = frs_2.iter().zip(&frs_1).map(|(x,y)| *x*y).sum();
        frs_2.push(-partial_sum/frs_1[3]);

        assert_eq!(
            frs_2.iter().zip(&frs_1).map(|(x,y)| *x*y).sum::<Fr>(),
            Fr::zero()
        );

        let g1s : Vec<G1Projective> = frs_1.into_iter().map(|x| G1Projective::generator()*x).collect();
        let g2s : Vec<G2Projective> = frs_2.into_iter().map(|x| G2Projective::generator()*x).collect();

        MultipairingEquation::new(g1s, g2s)
    }

    fn make_eq_nonzero() -> MultipairingEquation<Pairing> {
        let mut rng = thread_rng();

        let mut frs_1 = Vec::new();
        for _ in 0..4 {
            frs_1.push(Fr::rand(&mut rng));
        }

        let mut frs_2 = Vec::new();
        for _ in 0..4 {
            frs_2.push(Fr::rand(&mut rng));
        }


        assert!(
            frs_2.iter().zip(&frs_1).map(|(x,y)| *x*y).sum::<Fr>()
            !=
            Fr::zero()
        );

        let g1s : Vec<G1Projective> = frs_1.into_iter().map(|x| G1Projective::generator()*x).collect();
        let g2s : Vec<G2Projective> = frs_2.into_iter().map(|x| G2Projective::generator()*x).collect();

        MultipairingEquation::new(g1s, g2s)
    }

    #[test]
    fn test_single_multipairing_eq() {
        let eq = make_eq();
        eq.equals_zero().unwrap();
    }

    #[test]
    #[should_panic]
    fn test_single_multipairing_eq_nonzero() {
        let eq = make_eq_nonzero();

        eq.equals_zero().unwrap();
    }

    #[test]
    fn test_two_multipairing_eq() {
        let mut rng = thread_rng();
        let eq1 = make_eq();
        let eq2 = make_eq();

        eq1.combine(&mut rng, eq2).equals_zero().unwrap();
    }

    #[test]
    #[should_panic]
    fn test_two_multipairing_eq_nonzero() {
        let mut rng = thread_rng();
        let eq1 = make_eq();
        let eq2 = make_eq_nonzero();

        eq1.combine(&mut rng, eq2).equals_zero().unwrap();
    }

    #[test]
    #[should_panic]
    fn test_two_multipairing_eq_nonzero_2() {
        let mut rng = thread_rng();
        let eq1 = make_eq_nonzero();
        let eq2 = make_eq();

        eq1.combine(&mut rng, eq2).equals_zero().unwrap();
    }

    #[test]
    #[should_panic]
    fn test_two_multipairing_eq_nonzero_3() {
        let mut rng = thread_rng();
        let eq1 = make_eq_nonzero();
        let eq2 = make_eq_nonzero();

        eq1.combine(&mut rng, eq2).equals_zero().unwrap();
    }
}

