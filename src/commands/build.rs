use crate::utils::print_args::print_args;

pub fn build(args: &[String]) {
    println!("build was matched");
    print_args(args);
}
