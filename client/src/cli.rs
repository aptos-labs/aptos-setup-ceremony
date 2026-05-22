
use clap::{Parser, Subcommand};

pub const KEYPAIR_FILE : &str = "keypair.json";

#[derive(Parser)]
#[command(name = "aptos-setup-ceremony", about = "Use this program to contribute to the Aptos setup ceremony.")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    GenerateKeypair {
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        email: String,
        #[arg(short, long)]
        force: bool,
    },

    Identify {
        contributor_keypair_hex: String,
        #[arg(short, long)]
        force: bool,
    },

    Contribute,

    Verify {
        current_file: String,
        previous_file: Option<String>,
    },

    ComputeOutput {
        contribution_file: String,
        #[arg(short, long)]
        truncate: Option<usize>,
        #[arg(short, long)]
        smoke_test_one: bool,
        #[arg(short, long)]
        smoke_test_all: bool,
    },

    Admin {
        #[command(subcommand)]
        command: AdminCommand
    },
}

#[derive(Subcommand)]
pub enum AdminCommand {
    RegisterAll {
        #[arg(short, long)]
        file: String,
    },
    GenerateAllKeypairs {
        #[arg(short, long)]
        file: String,
    },
    Report,
    DownloadAll,
    SmokeTestLatest {
        #[arg(short, long)]
        truncate: Option<usize>,
        #[arg(short, long)]
        all: bool,
    },
}
