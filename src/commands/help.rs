use crate::utils::print_args::print_args;

pub fn help(args: &[String]) {
    println!("help was matched/called");
    print_args(args);
}
