//! Repository-wide unfinished-work audit.

use std::path::{Path, PathBuf};

use crate::Fail;

/// Rejects unfinished capability markers outside quoted examples and code spans.
pub fn audit_deferral(root: &Path) -> Result<(), Fail> {
    let markers = [
        concat!("TO", "DO"),
        concat!("FIX", "ME"),
        concat!("unimplemented", "!"),
        concat!("to", "do!"),
        concat!("for ", "now"),
        concat!("later ", "version"),
    ];
    let mut files = Vec::new();
    gather(root, &mut files)?;
    files.sort();

    let mut violations = Vec::new();
    for path in files {
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let relative = path.strip_prefix(root).unwrap_or(&path);
        for (index, line) in text.lines().enumerate() {
            let visible = outside_quotes_and_code_spans(line);
            for marker in markers {
                if visible.contains(marker) {
                    violations.push(format!(
                        "{}:{}: {}",
                        relative.display(),
                        index + 1,
                        line.trim()
                    ));
                }
            }
        }
    }

    if !violations.is_empty() {
        return Err(format!(
            "unfinished capability markers are not accepted:\n{}",
            violations.join("\n")
        )
        .into());
    }

    println!("audit-deferral: no unfinished capability marker was found");
    Ok(())
}

fn gather(dir: &Path, output: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| {
                name == "target" || name == ".git" || name == ".idea" || name == ".venv"
            }) {
                continue;
            }
            gather(&path, output)?;
        } else if path.extension().is_some_and(|extension| {
            matches!(
                extension.to_str(),
                Some("rs" | "md" | "toml" | "feature" | "yml" | "yaml")
            )
        }) {
            output.push(path);
        }
    }
    Ok(())
}

fn outside_quotes_and_code_spans(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut in_double_quote = false;
    let mut in_backtick = false;
    let mut escaped = false;

    for character in line.chars() {
        if in_double_quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_double_quote = false;
            }
            output.push(' ');
        } else if in_backtick {
            if character == '`' {
                in_backtick = false;
            }
            output.push(' ');
        } else if character == '"' {
            in_double_quote = true;
            output.push(' ');
        } else if character == '`' {
            in_backtick = true;
            output.push(' ');
        } else {
            output.push(character);
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::outside_quotes_and_code_spans;

    #[test]
    fn quoted_marker_is_not_a_deferral() {
        assert!(!outside_quotes_and_code_spans("`TODO` is a named marker").contains("TODO"));
    }

    #[test]
    fn visible_marker_remains_visible() {
        assert!(outside_quotes_and_code_spans("TODO implement it").contains("TODO"));
    }
}
