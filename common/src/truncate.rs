use crate::{combined::CombinedContributionInner, contribution::{Contribution, ContributionInner}, trx::{TRXContributionInner, M2C}, powers_of_tau::PowersOfTauContributionInner};
use batch_encryption::group::Pairing;



pub trait Truncate {
    fn truncate(&mut self, len: usize);
}


impl Truncate for PowersOfTauContributionInner<Pairing, M2C> {
    fn truncate(&mut self, len: usize) {
        self.powers.truncate(len + 1);
    }
}

impl Truncate for TRXContributionInner {
    fn truncate(&mut self, len: usize) {
        self.tau_powers_contrib_inner.truncate(len);
        for shifted_powers in &mut self.randomized_tau_powers_g1 {
            shifted_powers.truncate(len + 1);
        }
    }
}

impl Truncate for CombinedContributionInner {
    fn truncate(&mut self, len: usize) {
        self.trx.truncate(len);
    }
}

impl<I: ContributionInner + Truncate> Truncate for Contribution<I> {
    fn truncate(&mut self, len: usize) {
        self.inner.truncate(len);
    }
}
