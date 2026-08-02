//! Typed registries parsed from `model/*.toml`.
//!
//! The model is authored once and has exactly one source: the conformance ID
//! register, the claim ledger, and the authorities this repository cites.
//! `CONFORMANCE.md` is generated from it by [`codegen`].
//!
//! This crate is build-time and CI infrastructure. It is not a dependency of
//! the shipped crate and may use `std`.

#![deny(missing_docs)]

pub mod codegen;
pub mod registry;

pub use registry::{Authorities, AuthorityRow, Claim, IdRow, Ids, Ledger, Level};

use std::path::{Path, PathBuf};

const MODEL_SPEC: &str = "uor-semantic/1";

/// Everything `model/*.toml` says, parsed and cross-checked.
#[derive(Debug, Clone)]
pub struct Model {
    /// `model/ledger.toml`: claims that are not conformance IDs.
    pub ledger: Ledger,
    /// `model/ids.toml`: the conformance ID register.
    pub ids: Ids,
    /// `model/authorities.toml`: sources cited rather than established here.
    pub authorities: Authorities,
}

/// A failure to load or cross-check the model.
#[derive(Debug)]
pub enum ModelError {
    /// A model file could not be read.
    Io(PathBuf, std::io::Error),
    /// A model file could not be parsed.
    Parse(PathBuf, toml::de::Error),
    /// The model disagrees with itself.
    Inconsistent(String),
}

impl std::fmt::Display for ModelError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(path, error) => write!(formatter, "reading {}: {error}", path.display()),
            Self::Parse(path, error) => write!(formatter, "parsing {}: {error}", path.display()),
            Self::Inconsistent(message) => {
                write!(formatter, "model is inconsistent: {message}")
            }
        }
    }
}

impl std::error::Error for ModelError {}

impl Model {
    /// Loads every model file from a `model/` directory.
    pub fn load(dir: &Path) -> Result<Self, ModelError> {
        Ok(Self {
            ledger: read(dir, "ledger.toml")?,
            ids: read(dir, "ids.toml")?,
            authorities: read(dir, "authorities.toml")?,
        })
    }

    /// Loads the model relative to the repository root.
    pub fn load_from_repo_root() -> Result<Self, ModelError> {
        Self::load(&repo_root().join("model"))
    }

    /// Cross-checks IDs, levels, suites, and cited authorities.
    pub fn check(&self) -> Result<(), ModelError> {
        self.check_spec("model/ids.toml", &self.ids.spec)?;
        self.check_spec("model/ledger.toml", &self.ledger.spec)?;
        self.check_spec("model/authorities.toml", &self.authorities.spec)?;
        self.ledger.check()?;
        self.check_ids()?;
        self.check_authorities()?;
        Ok(())
    }

    fn check_spec(&self, file: &str, observed: &str) -> Result<(), ModelError> {
        if observed == MODEL_SPEC {
            return Ok(());
        }
        Err(ModelError::Inconsistent(format!(
            "{file}: expected schema `{MODEL_SPEC}`, found `{observed}`"
        )))
    }

    fn check_ids(&self) -> Result<(), ModelError> {
        let inconsistent = |message: String| ModelError::Inconsistent(message);
        let mut seen: Vec<&str> = Vec::new();
        for row in &self.ids.id {
            if seen.contains(&row.id.as_str()) {
                return Err(inconsistent(format!("{}: registered twice", row.id)));
            }
            seen.push(&row.id);

            if !valid_id(&row.id) {
                return Err(inconsistent(format!(
                    "{}: IDs use two uppercase letters, a hyphen, and two digits",
                    row.id
                )));
            }
            if row.statement.trim().is_empty() {
                return Err(inconsistent(format!(
                    "{}: an untagged claim does not ship",
                    row.id
                )));
            }
            if row.suite.trim().is_empty() {
                return Err(inconsistent(format!(
                    "{}: every ID names its Gherkin suite",
                    row.id
                )));
            }
        }
        Ok(())
    }

    fn check_authorities(&self) -> Result<(), ModelError> {
        let inconsistent = |message: String| ModelError::Inconsistent(message);
        let mut seen_authorities: Vec<&str> = Vec::new();
        for authority in &self.authorities.authority {
            if seen_authorities.contains(&authority.id.as_str()) {
                return Err(inconsistent(format!(
                    "{}: authority registered twice",
                    authority.id
                )));
            }
            seen_authorities.push(&authority.id);
            if authority.name.trim().is_empty() || authority.statement.trim().is_empty() {
                return Err(inconsistent(format!(
                    "{}: authority name and statement must be non-empty",
                    authority.id
                )));
            }
            if authority.citation.trim().is_empty() {
                return Err(inconsistent(format!(
                    "{}: an authority has no citation",
                    authority.id
                )));
            }
            if authority.checksum == "none" && authority.checksum_reason.trim().is_empty() {
                return Err(inconsistent(format!(
                    "{}: no checksum and no stated reason",
                    authority.id
                )));
            }
            for id in &authority.realized_by {
                if self.ids.get(id).is_none() {
                    return Err(inconsistent(format!(
                        "{}: realized_by names unknown ID {id}",
                        authority.id
                    )));
                }
            }
        }

        for claim in &self.ledger.claim {
            if claim.level != Level::SomeTrue {
                continue;
            }
            let Some(name) = &claim.authority else {
                return Err(inconsistent(format!(
                    "{}: a some-true claim must name an authority",
                    claim.id
                )));
            };
            if !self
                .authorities
                .authority
                .iter()
                .any(|authority| &authority.id == name)
            {
                return Err(inconsistent(format!(
                    "{}: cites {name}, which has no authority row",
                    claim.id
                )));
            }
        }
        Ok(())
    }
}

fn valid_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    bytes.len() == 5
        && bytes[0].is_ascii_uppercase()
        && bytes[1].is_ascii_uppercase()
        && bytes[2] == b'-'
        && bytes[3].is_ascii_digit()
        && bytes[4].is_ascii_digit()
}

fn read<T: serde::de::DeserializeOwned>(dir: &Path, name: &str) -> Result<T, ModelError> {
    let path = dir.join(name);
    let text =
        std::fs::read_to_string(&path).map_err(|error| ModelError::Io(path.clone(), error))?;
    toml::from_str(&text).map_err(|error| ModelError::Parse(path, error))
}

/// Returns the repository root resolved from this crate's manifest directory.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/model is two levels below the repository root")
        .to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::Model;

    /// CM-01: the model is self-consistent.
    #[test]
    fn model_is_consistent_cm_01() {
        let model = Model::load_from_repo_root().expect("model loads");
        model.check().expect("model checks");
    }

    /// CM-02: every registered ID is unique and well formed.
    #[test]
    fn the_id_register_is_well_formed_cm_02() {
        let model = Model::load_from_repo_root().expect("model loads");
        model.check().expect("model checks");
        assert!(!model.ids.id.is_empty(), "the populated repository has IDs");
    }

    /// CM-03: every cited authority is internally valid.
    #[test]
    fn every_cited_authority_is_well_formed_cm_03() {
        let model = Model::load_from_repo_root().expect("model loads");
        model.check().expect("model checks");
        assert!(
            !model.authorities.authority.is_empty(),
            "the repository records its design authorities"
        );
    }
}
