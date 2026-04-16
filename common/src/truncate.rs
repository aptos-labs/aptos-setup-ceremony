use crate::{aptos::AptosContributionInner, contribution::{Contribution, ContributionInner}, fptx::{FPTXContributionInner, M2C}, powers_of_tau::PowersOfTauContributionInner};
use aptos_batch_encryption::group::Pairing;



pub trait Truncate {
    fn truncate(&mut self, len: usize);
}


impl Truncate for PowersOfTauContributionInner<Pairing, M2C> {
    fn truncate(&mut self, len: usize) {
        self.powers.truncate(len);
    }
}

impl Truncate for FPTXContributionInner {
    fn truncate(&mut self, len: usize) {
        self.tau_powers_contrib_inner.truncate(len);
        for shifted_powers in &mut self.randomized_tau_powers_g1 {
            shifted_powers.truncate(len);
        }
    }
}

impl Truncate for AptosContributionInner {
    fn truncate(&mut self, len: usize) {
        self.fptx.truncate(len);
    }
}

impl<I: ContributionInner + Truncate> Truncate for Contribution<I> {
    fn truncate(&mut self, len: usize) {
        self.inner.truncate(len);
    }
}
