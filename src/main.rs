#![warn(clippy::all, clippy::pedantic)]
use crate::core::validate_command;
use std::env;

pub mod build;
pub mod core;
pub mod format;
pub mod help;
pub mod lint;
pub mod test;
pub mod utils;

fn main() {
    // Collect the arguments
    let args: Vec<String> = env::args().collect();
    // print out the arguments for debugging purposes
    println!("{:?}", args);
    // verify that there is a valid argument provided
    validate_command::validate_command(args)
}
