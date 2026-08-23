use clap::Parser;

/// mabel: peer-to-peer identity ledgers over Iroh.
#[derive(Parser)]
#[command(name = "mabel", version)]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
    eprintln!("mabel: commands land with the implementation tickets");
    std::process::exit(70);
}
