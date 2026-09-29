//! `synabit-safe run -- <command>`: run a command with secrets from Synabit's
//! Safe in its environment. The protocol and everything else is in
//! `safe/cli.rs`, compiled here on its own so this binary is small and does
//! not carry the app.

#[path = "../safe/cli.rs"]
#[allow(dead_code)]
mod cli;

#[cfg(unix)]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(cli::run(&args, true));
}

#[cfg(not(unix))]
fn main() {
    eprintln!("synabit-safe: this platform is not supported yet");
    std::process::exit(2);
}
