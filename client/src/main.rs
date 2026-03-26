use clap::{Parser, Subcommand};
use common::contribution::Contributor;
use rand::thread_rng;

#[derive(Parser)]
#[command(name = "aptos-setup-ceremony", about = "Use this program to contribute to the Aptos setup ceremony.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    GenerateKeyPair {
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        email: String,
    },

    Identify {
        contributor_hex: String,
    },

    Contribute,

    Admin {
        #[command(subcommand)]
        command: AdminCommand
    },
}

#[derive(Subcommand)]
enum AdminCommand {
    Register {
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        email: String,
        #[arg(short, long)]
        verifying_key_hex: Option<String>,
    },
    Report,
    DownloadAll
}

fn main() {
    let cli = Cli::parse();
    let mut rng = thread_rng();
    let (sk, c)  = Contributor::new("Rex Fernando", "rex1fernando@gmail.com", &mut rng);

    println!("{}", c.as_hex().unwrap());
    assert_eq!(c, Contributor::from_hex(&c.as_hex().unwrap()).unwrap());

}
