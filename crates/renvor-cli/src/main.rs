//! The `renvor` executable.

mod config;
mod exit;
mod generate;
mod paths;

fn main() {
    std::process::exit(exit::Exit::Success.code());
}
