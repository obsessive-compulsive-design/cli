pub fn print_args(args: &[String]) {
    println!("{args:?}");
}

#[cfg(test)]
mod tests {
    use super::print_args;

    #[test]
    fn prints_without_panicking_for_empty_and_populated_argv() {
        print_args(&[]);
        print_args(&["sjs".to_owned(), "lint".to_owned()]);
    }
}
