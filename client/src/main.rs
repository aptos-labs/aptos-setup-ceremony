pub mod run;
pub mod cli;
pub mod csv;


use clap::Parser;
use dirs;

use crate::cli::Cli;


fn main() {
    let cli = Cli::parse();
    let config_dir = dirs::config_dir()
        .map(|d| d.join("aptos-setup-ceremony"))
        .expect("no config dir found");
        
    std::fs::create_dir_all(&config_dir).expect("Should be able to create dir");

    run::run(cli, config_dir).unwrap();
}
