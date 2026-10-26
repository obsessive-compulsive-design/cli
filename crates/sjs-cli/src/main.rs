//! The `sjs` binary. All behaviour lives in the library so it can be unit-tested; this
//! file only connects the process to it.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args = std::env::args_os().skip(1).collect();
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    ExitCode::from(sjs_cli::run(args, &mut stdout.lock(), &mut stderr.lock()))
}
