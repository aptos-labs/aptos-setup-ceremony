//! The only reason for this file is that we don't want to require M2C to be
//! `{Debug,Clone,PartialEq,Eq}` in order to implement the respective traits for our structs that
//! are parameterized by it, and the default derive macros have this requirement.
use std::ops::Neg;

use ark_ec::{AffineRepr, hashing::map_to_curve_hasher::MapToCurve, pairing::Pairing};

use crate::{bls_sok::BLSSoK, hiding_kzg::HidingKZGContributionInner, powers_of_tau::PowersOfTauContributionInner};

impl<P: Eq, M2C> Eq for BLSSoK<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G1>,
    P::G2Affine: AffineRepr + Neg<Output = P::G2Affine>,
    P::G1Affine: AffineRepr,
{
}

impl<P: PartialEq, M2C> PartialEq for BLSSoK<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G1>,
    P::G2Affine: AffineRepr + Neg<Output = P::G2Affine>,
    P::G1Affine: AffineRepr,
{
    fn eq(&self, other: &Self) -> bool {
        self.sig == other.sig 
    }
}

impl<P: Clone, M2C> Clone for BLSSoK<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G1>,
    P::G2Affine: AffineRepr + Neg<Output = P::G2Affine>,
    P::G1Affine: AffineRepr,
{
    fn clone(&self) -> Self {
        Self { sig: self.sig, _phantom: self._phantom }
    }
}

impl<P: std::fmt::Debug, M2C> std::fmt::Debug for BLSSoK<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G1>,
    P::G2Affine: AffineRepr + Neg<Output = P::G2Affine>,
    P::G1Affine: AffineRepr,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BLSSoK").field("sig", &self.sig).field("_phantom", &self._phantom).finish()
    }
}

impl<P: Eq, M2C> Eq for PowersOfTauContributionInner<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G1>,
    P::G2Affine: AffineRepr + Neg<Output = P::G2Affine>,
    P::G1Affine: AffineRepr,
{
}

impl<P: PartialEq, M2C> PartialEq for PowersOfTauContributionInner<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G1>,
    P::G2Affine: AffineRepr + Neg<Output = P::G2Affine>,
    P::G1Affine: AffineRepr,
{
    fn eq(&self, other: &Self) -> bool {
        self.powers == other.powers && self.tau_g2 == other.tau_g2 && self.sok == other.sok
    }
}

impl<P: Clone, M2C> Clone for PowersOfTauContributionInner<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G1>,
    P::G2Affine: AffineRepr + Neg<Output = P::G2Affine>,
    P::G1Affine: AffineRepr,
{
    fn clone(&self) -> Self {
        Self { powers: self.powers.clone(), tau_g2: self.tau_g2, sok: self.sok.clone() }
    }
}

impl<P: std::fmt::Debug, M2C> std::fmt::Debug for PowersOfTauContributionInner<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G1>,
    P::G2Affine: AffineRepr + Neg<Output = P::G2Affine>,
    P::G1Affine: AffineRepr,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PowersOfTauContributionInner").field("powers", &self.powers).field("tau_g2", &self.tau_g2).field("sok", &self.sok).finish()
    }
}

impl<P: Eq, M2C> Eq for HidingKZGContributionInner<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G1>,
    P::G2Affine: AffineRepr + Neg<Output = P::G2Affine>,
    P::G1Affine: AffineRepr,
{
}

impl<P: PartialEq, M2C> PartialEq for HidingKZGContributionInner<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G1>,
    P::G2Affine: AffineRepr + Neg<Output = P::G2Affine>,
    P::G1Affine: AffineRepr,
{
    fn eq(&self, other: &Self) -> bool {
        self.tau_powers_contrib_inner == other.tau_powers_contrib_inner && self.xi_g1 == other.xi_g1 && self.xi_g2 == other.xi_g2 && self.sok_xi == other.sok_xi
    }
}

impl<P: Clone, M2C> Clone for HidingKZGContributionInner<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G1>,
    P::G2Affine: AffineRepr + Neg<Output = P::G2Affine>,
    P::G1Affine: AffineRepr,
{
    fn clone(&self) -> Self {
        Self {
            tau_powers_contrib_inner: self.tau_powers_contrib_inner.clone(),
            xi_g1: self.xi_g1,
            xi_g2: self.xi_g2,
            sok_xi: self.sok_xi.clone(),
        }
    }
}

impl<P: std::fmt::Debug, M2C> std::fmt::Debug for HidingKZGContributionInner<P, M2C>
where
    P: Pairing,
    M2C: MapToCurve<P::G1>,
    P::G2Affine: AffineRepr + Neg<Output = P::G2Affine>,
    P::G1Affine: AffineRepr,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HidingKZGContributionInner")
            .field("tau_powers_contrib_inner", &self.tau_powers_contrib_inner)
            .field("xi_g1", &self.xi_g1)
            .field("xi_g2", &self.xi_g2)
            .field("sok_xi", &self.sok_xi)
            .finish()
    }
}
