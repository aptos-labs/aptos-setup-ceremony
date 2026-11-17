
use std::fs;

use rand::thread_rng;

use common::{contribution::ContributionInner, trx::{TRXContributionInner, TRXParams}};

fn main() {
    let mut rng = thread_rng();
    let params = TRXParams::new(128, 216000).unwrap();

    let time = std::time::Instant::now();
    println!("Starting \"dummy\" first contrib at {}", chrono::Local::now() );
    let first_contrib = TRXContributionInner::first_contribution(&params);
    println!("Time to generate \"dummy\" first contrib: {:?}", time.elapsed());
    let time = std::time::Instant::now();
    println!("Starting user contrib at {}", chrono::Local::now() );
    let (new_contrib, _) = TRXContributionInner::generate(&mut rng, &first_contrib, &params).unwrap();
    println!("Time to generate a user contrib: {:?}", time.elapsed());

    let time = std::time::Instant::now();
    println!("Starting user contrib serialization at {}", chrono::Local::now() );
    fs::write("./test.contrib", bcs::to_bytes(&new_contrib).unwrap()).unwrap();
    println!("Time to serialize a user contrib: {:?}", time.elapsed());
    println!("Size: {} MB", fs::metadata("./test.contrib").unwrap().len()/1000/1000);
    
    drop(new_contrib);

    let time = std::time::Instant::now();
    println!("Starting user contrib deserialization at {}", chrono::Local::now() );
    let new_contrib : TRXContributionInner = bcs::from_reader(fs::File::open("./test.contrib").unwrap()).unwrap();
    println!("Time to deserialize a user contrib: {:?}", time.elapsed());


    let time = std::time::Instant::now();
    println!("Starting user contrib verification at {}", chrono::Local::now() );
    new_contrib.verify(&mut rng, &first_contrib, &params)
        .unwrap()
        .equals_zero()
        .unwrap();
    println!("Time to verify a user contrib: {:?}", time.elapsed());
}
