pub mod verification_job;
pub mod store;
pub mod authentication;
pub mod handlers;
pub mod messages;
pub mod error;

pub type Request = hyper::Request<hyper::body::Incoming>;

fn main() {
    println!("Hello, world!");
}
