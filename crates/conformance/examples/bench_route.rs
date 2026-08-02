//! Emit an empirical semantic-routing benchmark report.

use std::error::Error;

use repo_conformance::{BenchmarkConfig, run_route_benchmark};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args().skip(1);
    let samples = parse_or(arguments.next(), 31, "samples")?;
    let iterations = parse_or(arguments.next(), 10_000, "iterations")?;
    if let Some(extra) = arguments.next() {
        return Err(format!("unexpected argument `{extra}`").into());
    }

    let config = BenchmarkConfig::new(samples, iterations)?;
    let report = run_route_benchmark(config)?;
    println!("{}", report.to_json());
    Ok(())
}

fn parse_or(value: Option<String>, fallback: usize, label: &str) -> Result<usize, Box<dyn Error>> {
    match value {
        Some(value) => value
            .parse::<usize>()
            .map_err(|error| format!("invalid {label} `{value}`: {error}").into()),
        None => Ok(fallback),
    }
}
