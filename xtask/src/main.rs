//! Repository acceptance gates.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use repo_conformance::audit_strict_core;
use repo_model::{Model, codegen};

mod audit;

type Fail = Box<dyn std::error::Error>;

fn main() -> ExitCode {
    let task = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "help".to_string());
    let write = std::env::args().any(|argument| argument == "--write");
    let root = repo_model::repo_root();

    let result = match task.as_str() {
        "check-model" => check_model(&root, write),
        "audit-core" => audit_core(&root),
        "audit-deferral" => audit::audit_deferral(&root),
        "validate" => validate(&root),
        _ => {
            eprintln!(
                "cargo xtask <task>\n\
                 \n\
                 check-model       regenerate and compare CONFORMANCE.md\n\
                 audit-core        audit the shipped strict runtime boundary\n\
                 audit-deferral    reject unfinished capability markers\n\
                 validate          run every repository gate\n\
                 \n\
                 --write           check-model only: rewrite generated output"
            );
            return ExitCode::from(2);
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("gate failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn check_model(root: &Path, write: bool) -> Result<(), Fail> {
    let model = Model::load(&root.join("model"))?;
    model.check()?;
    let generated = codegen::render_conformance(&model);
    let path: PathBuf = root.join(codegen::CONFORMANCE_PATH);

    if write {
        std::fs::write(&path, generated)?;
        println!("wrote {}", path.display());
        return Ok(());
    }

    let committed = std::fs::read_to_string(&path).map_err(|error| {
        format!(
            "{}: {error}; run `cargo xtask check-model --write`",
            path.display()
        )
    })?;
    if committed != generated {
        return Err(format!(
            "{} is stale relative to model/*.toml; run `cargo xtask check-model --write`",
            path.display()
        )
        .into());
    }

    println!(
        "check-model: generated conformance matches {} registered IDs",
        model.ids.id.len()
    );
    Ok(())
}

fn audit_core(root: &Path) -> Result<(), Fail> {
    let report = audit_strict_core(root)?;
    if !report.is_clean() {
        return Err(format!(
            "strict-core source audit failed:\n{}",
            report.violations.join("\n")
        )
        .into());
    }
    println!(
        "audit-core: {} production Rust files satisfy the strict source contract",
        report.files_scanned
    );
    Ok(())
}

fn validate(root: &Path) -> Result<(), Fail> {
    check_model(root, false)?;
    audit_core(root)?;
    audit::audit_deferral(root)?;
    println!("validate: every repository gate passed");
    Ok(())
}
