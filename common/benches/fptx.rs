
use rand::thread_rng;

use common::{contribution::ContributionInner, fptx::{FPTXContributionInner, FPTXParams}};

fn main() {
        let mut rng = thread_rng();
        let params = FPTXParams::new(128, 4).unwrap();

        let first_contrib = FPTXContributionInner::first_contribution(&params);
        let (new_contrib, _) = FPTXContributionInner::generate(&mut rng, &first_contrib, &params);

        new_contrib.verify(&mut rng, &first_contrib, &params)
            .unwrap()
            .equals_zero()
            .unwrap();
}
