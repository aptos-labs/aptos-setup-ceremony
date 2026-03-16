use ark_ec::{
    VariableBaseMSM,
    pairing::{Pairing, PairingOutput},
};
use ark_ff::Zero;
use ark_std::UniformRand;
use rand_core::CryptoRngCore;
use rayon::iter::{
    IndexedParallelIterator, IntoParallelIterator, IntoParallelRefIterator, ParallelIterator,
};

use crate::errors::MultipairingEquationNonZeroResult;

pub struct MultipairingEquation<P>
where
    P: Pairing,
{
    // note: this struct is never serialized, so normalization doesn't happen
    g1s: Vec<P::G1Affine>,
    g2s: Vec<P::G2Affine>,
}

impl<P> MultipairingEquation<P>
where
    P: Pairing,
{
    /// Construct a new multipairing equation. Takes vecs instead of slices so that we can move w/o
    /// calling `to_vec()`, which I believe does a copy.
    pub fn simple(g1s: Vec<P::G1Affine>, g2s: Vec<P::G2Affine>) -> Self {
        Self { g1s, g2s }
    }

    pub fn with_shared_g2s(
        rng: &mut impl CryptoRngCore,
        g1s: Vec<Vec<P::G1Affine>>,
        shared_g2s: Vec<P::G2Affine>,
    ) -> Self {
        assert!(g1s.len() > 0);

        let random_challenge: Vec<P::ScalarField> =
            (0..g1s.len()).map(|_| P::ScalarField::rand(rng)).collect();

        // I believe that having g1s,g2s be G1 instead of G1Affine means less work here since we
        // don't have to normalize.
        let g1s_combined_scaled: Vec<P::G1Affine> = (0..g1s[0].len())
            .map(|i| {
                <P::G1 as VariableBaseMSM>::msm_unchecked(
                    &g1s.iter()
                        .map(|g1s| g1s[i])
                        .collect::<Vec<P::G1Affine>>(),
                    &random_challenge,
                ).into()
            })
            .collect();

        Self {
            g1s: g1s_combined_scaled,
            g2s: shared_g2s,
        }
    }

    pub fn empty() -> Self {
        Self::simple(vec![], vec![])
    }

    fn scalar_mul(self, scalar: P::ScalarField) -> Self {
        Self {
            g1s: self.g1s.into_par_iter().map(|g1| P::G1Affine::from(g1 * scalar)).collect(),
            // I believe the move and the use of `self` instead of `&self` above means no copy here
            g2s: self.g2s,
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
        println!("multipairing size: {}", self.g1s.len());
        let time = std::time::Instant::now();
        let g1s_prepared: Vec<P::G1Prepared> = self
            .g1s
            .par_iter()
            .map(|g| P::G1Prepared::from(*g))
            .collect();
        let g2s_prepared: Vec<P::G2Prepared> = self
            .g2s
            .par_iter()
            .map(|g| P::G2Prepared::from(*g))
            .collect();
        let output = P::multi_pairing(g1s_prepared, g2s_prepared);
        println!("equals_zero: {:?}", time.elapsed());

        if output == PairingOutput::zero() {
            Ok(())
        } else {
            Err(MultipairingEquationNonZeroResult)
        }
    }
}

pub struct MultipairingEquations<P: Pairing> {
    pub eqns: Vec<MultipairingEquation<P>>,
}

impl<P: Pairing> MultipairingEquations<P> {
    pub fn new() -> Self {
        Self { eqns: vec![] }
    }

    pub fn add(mut self, eq: MultipairingEquation<P>) -> Self {
        self.eqns.push(eq);
        self
    }

    pub fn add_eqs(mut self, mut eq: MultipairingEquations<P>) -> Self {
        self.eqns.append(&mut eq.eqns);
        self
    }

    pub fn compact(self, rng: &mut impl CryptoRngCore) -> MultipairingEquation<P> {
        println!("compact size: {}", self.eqns.len());
        let time = std::time::Instant::now();
        let mut random_scalars = Vec::new();
        for _ in 0..self.eqns.len() {
            random_scalars.push(P::ScalarField::rand(rng));
        }

        // TODO could do batch normalization here
        let (g1s, g2s): (Vec<<P as Pairing>::G1Affine>, Vec<<P as Pairing>::G2Affine>) = self
            .eqns
            .into_par_iter()
            .zip(random_scalars)
            .map(|(eq, scalar)| {
                let new_eq = eq.scalar_mul(scalar);
                (new_eq.g1s, new_eq.g2s)
            })
            .flatten()
            .collect();

        println!("compact: {:?}", time.elapsed());

        MultipairingEquation::simple(g1s, g2s)
    }
}

#[cfg(test)]
mod tests {
    use aptos_batch_encryption::group::{Fr, Pairing, G1Affine, G2Affine};
    use ark_ec::AffineRepr as _;
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

        let partial_sum: Fr = frs_2.iter().zip(&frs_1).map(|(x, y)| *x * y).sum();
        frs_2.push(-partial_sum / frs_1[3]);

        assert_eq!(
            frs_2.iter().zip(&frs_1).map(|(x, y)| *x * y).sum::<Fr>(),
            Fr::zero()
        );

        let g1s: Vec<G1Affine> = frs_1
            .into_iter()
            .map(|x| G1Affine::from(G1Affine::generator() * x))
            .collect();
        let g2s: Vec<G2Affine> = frs_2
            .into_iter()
            .map(|x| G2Affine::from(G2Affine::generator() * x))
            .collect();

        MultipairingEquation::simple(g1s, g2s)
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

        assert!(frs_2.iter().zip(&frs_1).map(|(x, y)| *x * y).sum::<Fr>() != Fr::zero());

        let g1s: Vec<G1Affine> = frs_1
            .into_iter()
            .map(|x| G1Affine::from(G1Affine::generator() * x))
            .collect();
        let g2s: Vec<G2Affine> = frs_2
            .into_iter()
            .map(|x| G2Affine::from(G2Affine::generator() * x))
            .collect();

        MultipairingEquation::simple(g1s, g2s)
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
    fn test_two_multipairing_eq_one_empty() {
        let mut rng = thread_rng();
        let eq1 = MultipairingEquation::empty();
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
