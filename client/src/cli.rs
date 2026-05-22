
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
    /// Generate a keypair file based on the given name + email. **This is not
    /// the normal flow; normally, users should use the "identify" command with an
    /// auth code that they received.** 
    GenerateKeypair {
        /// The contributor's name.
        #[arg(short, long)]
        name: String,
        /// The contributor's email.
        #[arg(short, long)]
        email: String,
        /// Use this to overwrite the curren't directory's keypair file.
        #[arg(short, long)]
        force: bool,
    },

    /// Authenticate using a provided auth code. Writes a keypair file.
    Identify {
        /// The auth code. You should have received this auth code from the ceremony admin.
        contributor_keypair_hex: String,
        #[arg(short, long)]
        force: bool,
    },

    /// Contribute to the ceremony.
    Contribute,

    /// Verify a contribution given the previous (or nothing if this is the first contribution).
    Verify {
        current_file: String,
        previous_file: Option<String>,
    },

    /// Compute the outputs of the ceremony based on a contribution.
    ComputeOutput {
        contribution_file: String,
        /// Optionally, truncate the max batch size to a given size.
        #[arg(short, long)]
        truncate: Option<usize>,
        /// Optionally, smoke test one round of batch threshold encryption.
        #[arg(short, long)]
        smoke_test_one: bool,
        /// Optionally, smoke test all rounds of batch threshold encryption.
        #[arg(short, long)]
        smoke_test_all: bool,
    },
    
    /// Subcommands for ceremony admin.
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
