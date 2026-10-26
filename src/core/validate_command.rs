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

#[cfg(test)]
mod tests {
    use super::validate_command;

    fn argv(rest: &[&str]) -> Vec<String> {
        std::iter::once("sjs")
            .chain(rest.iter().copied())
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn dispatches_every_known_command() {
        for command in ["build", "format", "help", "lint", "test"] {
            validate_command(&argv(&[command]));
        }
    }

    #[test]
    fn command_matching_ignores_case_and_surrounding_whitespace() {
        validate_command(&argv(&[" LINT "]));
    }

    #[test]
    fn unknown_command_is_reported() {
        validate_command(&argv(&["bogus"]));
    }

    #[test]
    fn missing_command_falls_back_to_help() {
        validate_command(&argv(&[]));
    }
}
