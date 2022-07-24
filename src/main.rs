#![warn(clippy::all, clippy::pedantic)]
use utils::print_args::print_args;

use crate::core::validate_command::validate_command;
use std::env;

pub mod commands;
pub mod core;
pub mod utils;

fn main() {
    // Collect the arguments
    let args: Vec<String> = env::args().collect();
    // print out the arguments for debugging purposes
    print_args(&args);
    // verify that there is a valid argument provided
    validate_command(&args);
}
