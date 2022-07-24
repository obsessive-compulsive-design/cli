use crate::build::build::build;
use crate::format::format::format;
use crate::help::help::help;
use crate::lint::lint::lint;
use crate::test::test::test;

pub fn validate_command(args: Vec<String>) {
    if args.len() > 1 {
        match args[1].to_lowercase().trim() {
            "build" => build(),
            "format" => format(),
            "help" => help(),
            "lint" => lint(),
            "test" => test(),
            _ => eprintln!("{} is not a valid command", args[1]),
        }
    } else {
        eprintln!("Please provide a command to the CLI. Defaulting to help.");
        help()
    }
}
