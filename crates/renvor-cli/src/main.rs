//! The `renvor` executable.

mod config;
mod exit;
mod generate;
mod output;
mod paths;

fn main() {
    std::process::exit(exit::Exit::Success.code());
}
