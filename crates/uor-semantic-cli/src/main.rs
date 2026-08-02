//! `uor-semantic` command-line entry point.

fn main() -> std::process::ExitCode {
    match uor_semantic_cli::run_env() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
