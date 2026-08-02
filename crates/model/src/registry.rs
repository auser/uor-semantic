//! Typed shapes of `model/*.toml`.

use serde::Deserialize;

use crate::ModelError;

/// One of the three honesty levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Level {
    /// A fact reproduced from an authority and not established here.
    SomeTrue,
    /// Constructed here and validated against its oracle.
    Build,
    /// Measured and reported, never asserted as universal.
    Open,
}

impl Level {
    /// Returns the canonical model token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SomeTrue => "some-true",
            Self::Build => "build",
            Self::Open => "open",
        }
    }
}

/// `model/ledger.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct Ledger {
    /// Schema tag.
    pub spec: String,
    /// Claims that are not conformance IDs.
    pub claim: Vec<Claim>,
}

/// One claim at exactly one honesty level.
#[derive(Debug, Clone, Deserialize)]
pub struct Claim {
    /// Stable claim identifier.
    pub id: String,
    /// Honesty level.
    pub level: Level,
    /// Human-readable statement.
    pub statement: String,
    /// Gherkin file carrying a build scenario.
    #[serde(default)]
    pub feature: Option<String>,
    /// Authority cited by a some-true claim.
    #[serde(default)]
    pub authority: Option<String>,
    /// Recorded sample size for a statistic.
    #[serde(default)]
    pub sample_size: Option<u64>,
    /// Recorded seed for a statistic.
    #[serde(default)]
    pub seed: Option<u64>,
}

impl Ledger {
    /// Checks level-specific structural requirements.
    pub fn check(&self) -> Result<(), ModelError> {
        let mut seen: Vec<&str> = Vec::new();
        for claim in &self.claim {
            if seen.contains(&claim.id.as_str()) {
                return Err(ModelError::Inconsistent(format!(
                    "{}: claim registered twice",
                    claim.id
                )));
            }
            seen.push(&claim.id);
            if claim.statement.trim().is_empty() {
                return Err(ModelError::Inconsistent(format!(
                    "{}: claim statement must be non-empty",
                    claim.id
                )));
            }
            match claim.level {
                Level::SomeTrue => {
                    if claim.authority.is_none() {
                        return Err(ModelError::Inconsistent(format!(
                            "{}: a some-true claim must name its authority",
                            claim.id
                        )));
                    }
                }
                Level::Build => {
                    if claim.feature.is_none() {
                        return Err(ModelError::Inconsistent(format!(
                            "{}: a build claim must name its Gherkin feature",
                            claim.id
                        )));
                    }
                    if claim.authority.is_some() {
                        return Err(ModelError::Inconsistent(format!(
                            "{}: a build claim must not claim an upstream authority",
                            claim.id
                        )));
                    }
                }
                Level::Open => {
                    if claim.authority.is_some() {
                        return Err(ModelError::Inconsistent(format!(
                            "{}: an open measurement cannot cite an authority for its value",
                            claim.id
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    /// Looks up a claim by identifier.
    pub fn get(&self, id: &str) -> Option<&Claim> {
        self.claim.iter().find(|claim| claim.id == id)
    }
}

/// `model/ids.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct Ids {
    /// Schema tag.
    pub spec: String,
    /// Registered conformance IDs.
    pub id: Vec<IdRow>,
}

/// One registered conformance ID.
#[derive(Debug, Clone, Deserialize)]
pub struct IdRow {
    /// Stable ID, such as `SR-01`.
    pub id: String,
    /// Honesty level.
    pub level: Level,
    /// Gherkin suite name.
    pub suite: String,
    /// Human-readable behavior statement.
    pub statement: String,
}

impl Ids {
    /// Looks up an ID row.
    pub fn get(&self, id: &str) -> Option<&IdRow> {
        self.id.iter().find(|row| row.id == id)
    }
}

/// `model/authorities.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct Authorities {
    /// Schema tag.
    pub spec: String,
    /// Cited authorities.
    pub authority: Vec<AuthorityRow>,
}

/// One cited authority.
#[derive(Debug, Clone, Deserialize)]
pub struct AuthorityRow {
    /// Stable authority identifier.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Stable citation.
    pub citation: String,
    /// Checksum or immutable revision marker.
    pub checksum: String,
    /// Reason no checksum is available.
    #[serde(default)]
    pub checksum_reason: String,
    /// What the authority contributes.
    pub statement: String,
    /// Conformance IDs that realize the local design choice.
    #[serde(default)]
    pub realized_by: Vec<String>,
}
