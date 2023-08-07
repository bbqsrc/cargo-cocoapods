use std::env;
use std::process::exit;

mod cargo;
mod cli;
mod cmd;
mod meta;
mod podspec;

pub(crate) static MACOS_TRIPLES: &[&str] = &[
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    // "x86_64-apple-ios-macabi",
];

pub(crate) static IOS_TRIPLES: &[&str] = &[
    "x86_64-apple-ios",
    "aarch64-apple-ios",
    "aarch64-apple-ios-sim",
];

#[tokio::main]
async fn main() {
    env_logger::from_env(env_logger::Env::default().default_filter_or("cargo_ndk=info")).init();

    if env::var("CARGO").is_err() {
        eprintln!("This binary may only be called via `cargo pod`.");
        exit(1);
    }

    let args = std::env::args().skip(2).collect::<Vec<_>>();

    cli::run(args).await;
}
