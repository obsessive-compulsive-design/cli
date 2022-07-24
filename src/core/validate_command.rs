use crate::commands::build::build;
use crate::commands::format::format;
use crate::commands::help::help;
use crate::commands::lint::lint;
use crate::commands::test::test;

pub fn validate_command(args: &[String]) {
    if args.len() > 1 {
        match args[1].to_lowercase().trim() {
            "build" => build(args),
            "format" => format(args),
            "help" => help(args),
            "lint" => lint(args),
            "test" => test(args),
            _ => eprintln!("{} is not a valid command", args[1]),
        }
    } else {
        eprintln!("Please provide a command to the CLI. Defaulting to help.");
        help(args);
    }
}
