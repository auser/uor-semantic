//! Structural audit of the shipped strict runtime crate.
//!
//! This audit is source evidence, not a machine-code proof. Its purpose is to
//! keep the declared Rust-level boundary honest and to fail immediately when a
//! forbidden dependency, type, operation, or escape hatch enters the core.

use core::fmt;
use std::path::{Path, PathBuf};

/// Failure to load or parse material required by the source audit.
#[derive(Debug)]
pub enum SourceAuditError {
    /// A source or manifest file could not be read.
    Io(PathBuf, std::io::Error),
    /// The strict-core manifest was malformed TOML.
    Manifest(toml::de::Error),
}

impl fmt::Display for SourceAuditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(path, error) => write!(formatter, "reading {}: {error}", path.display()),
            Self::Manifest(error) => write!(formatter, "parsing strict-core manifest: {error}"),
        }
    }
}

impl std::error::Error for SourceAuditError {}

/// Result of auditing the strict runtime source boundary.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SourceAuditReport {
    /// Rust source files inspected.
    pub files_scanned: usize,
    /// Human-readable violations, each naming its source location.
    pub violations: Vec<String>,
}

impl SourceAuditReport {
    /// Returns whether the audit found no violation.
    pub fn is_clean(&self) -> bool {
        self.violations.is_empty()
    }
}

/// Audits the published `uor-semantic` crate under `root`.
pub fn audit_strict_core(root: &Path) -> Result<SourceAuditReport, SourceAuditError> {
    let crate_root = root.join("crates/uor-semantic");
    let manifest_path = crate_root.join("Cargo.toml");
    let manifest_text = read(&manifest_path)?;
    let manifest: toml::Value =
        toml::from_str(&manifest_text).map_err(SourceAuditError::Manifest)?;

    let mut report = SourceAuditReport::default();
    let dependencies = manifest.get("dependencies").and_then(toml::Value::as_table);
    if dependencies.is_some_and(|table| !table.is_empty()) {
        report
            .violations
            .push("RT-01: the strict core must have no runtime dependencies".to_string());
    }
    let default_features = manifest
        .get("features")
        .and_then(toml::Value::as_table)
        .and_then(|features| features.get("default"))
        .and_then(toml::Value::as_array);
    if default_features.is_none_or(|features| !features.is_empty()) {
        report
            .violations
            .push("RT-01: the strict core must have an empty default feature set".to_string());
    }

    let lib_path = crate_root.join("src/lib.rs");
    let lib = read(&lib_path)?;
    for required in [
        "#![no_std]",
        "#![forbid(unsafe_code)]",
        "#![deny(clippy::alloc_instead_of_core)]",
        "#![deny(clippy::float_arithmetic)]",
        "#![deny(clippy::std_instead_of_core)]",
        "#![deny(clippy::expect_used)]",
        "#![deny(clippy::panic)]",
        "#![deny(clippy::unwrap_used)]",
    ] {
        if !lib.contains(required) {
            report.violations.push(format!(
                "RT-01: crates/uor-semantic/src/lib.rs is missing `{required}`"
            ));
        }
    }

    let mut files = Vec::new();
    collect_rs(&crate_root.join("src"), &mut files)
        .map_err(|error| SourceAuditError::Io(crate_root.join("src"), error))?;
    files.sort();
    for path in files {
        report.files_scanned += 1;
        let text = read(&path)?;
        let relative = path.strip_prefix(root).unwrap_or(&path);
        audit_text(relative, &text, &mut report.violations);
    }

    if report.files_scanned == 0 {
        report
            .violations
            .push("RT-01: the strict-core source audit scanned no files".to_string());
    }

    Ok(report)
}

fn read(path: &Path) -> Result<String, SourceAuditError> {
    std::fs::read_to_string(path).map_err(|error| SourceAuditError::Io(path.to_path_buf(), error))
}

fn collect_rs(dir: &Path, output: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_rs(&path, output)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            output.push(path);
        }
    }
    Ok(())
}

fn audit_text(path: &Path, text: &str, violations: &mut Vec<String>) {
    for (index, raw) in text.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = raw.trim();
        if trimmed.starts_with("//") || trimmed.is_empty() {
            continue;
        }
        if raw.contains("/*") || raw.contains("*/") {
            violations.push(format!(
                "RT-01: {}:{line_number}: block comments are disallowed in audited core source",
                path.display()
            ));
            continue;
        }

        let code = strip_strings(raw);
        let code = code.split("//").next().unwrap_or("").trim();
        if code.is_empty() {
            continue;
        }

        for token in [
            "extern crate alloc",
            "alloc::",
            "std::",
            "Vec<",
            "Vec ::",
            "String",
            "Box<",
            "Box ::",
            "f32",
            "f64",
            "panic!",
            "todo!",
            "unimplemented!",
        ] {
            if contains_token(code, token) {
                violations.push(format!(
                    "RT-01: {}:{line_number}: forbidden core token `{token}`",
                    path.display()
                ));
            }
        }

        if contains_token(code, "unsafe") && code != "#![forbid(unsafe_code)]" {
            violations.push(format!(
                "RT-01: {}:{line_number}: unsafe code is forbidden",
                path.display()
            ));
        }

        for operator in ['*', '/', '%'] {
            if code.contains(operator) {
                violations.push(format!(
                    "RT-01: {}:{line_number}: forbidden arithmetic operator `{operator}`",
                    path.display()
                ));
            }
        }
        for method in [
            ".mul(",
            "wrapping_mul(",
            "saturating_mul(",
            "checked_mul(",
            "pow(",
            "div_ceil(",
        ] {
            if code.contains(method) {
                violations.push(format!(
                    "RT-01: {}:{line_number}: forbidden arithmetic method `{method}`",
                    path.display()
                ));
            }
        }
    }
}

fn contains_token(code: &str, token: &str) -> bool {
    if token
        .bytes()
        .any(|byte| !byte.is_ascii_alphanumeric() && byte != b'_')
    {
        return code.contains(token);
    }

    let mut offset = 0usize;
    while let Some(found) = code[offset..].find(token) {
        let start = offset + found;
        let end = start + token.len();
        let before = code[..start].chars().next_back();
        let after = code[end..].chars().next();
        let boundary = |character: Option<char>| {
            character.is_none_or(|value| !value.is_ascii_alphanumeric() && value != '_')
        };
        if boundary(before) && boundary(after) {
            return true;
        }
        offset = end;
    }
    false
}

fn strip_strings(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let characters = line.chars();
    let mut in_string = false;
    let mut escaped = false;

    for character in characters {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            output.push(' ');
            continue;
        }

        if character == '"' {
            in_string = true;
            output.push(' ');
        } else {
            output.push(character);
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::audit_text;

    #[test]
    fn source_audit_detects_a_forbidden_product() {
        let mut violations = Vec::new();
        audit_text(
            Path::new("planted.rs"),
            "fn planted(a: u64, b: u64) -> u64 { a * b }",
            &mut violations,
        );
        assert!(violations.iter().any(|violation| violation.contains('`')));
    }

    #[test]
    fn source_audit_ignores_documentation_and_strings() {
        let mut violations = Vec::new();
        audit_text(
            Path::new("control.rs"),
            "/// multiplication uses `*` elsewhere\nconst MESSAGE: &str = \"a * b\";",
            &mut violations,
        );
        assert!(violations.is_empty());
    }
}
