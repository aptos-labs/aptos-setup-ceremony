use clap::{Parser, Subcommand};

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
    }
}

fn main() {
    let cli = Cli::parse();


}
