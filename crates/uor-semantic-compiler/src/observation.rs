//! Parsing and canonical representation of captured teacher observations.

use core::fmt;
use std::collections::BTreeSet;
use std::path::Path;

/// One captured token score from the source model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObservedEmission {
    /// Token identifier.
    pub token: u32,
    /// Signed fixed-point logit or residual score.
    pub score: i32,
}

/// One bounded context and the source model's next-token evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Observation {
    /// Context token IDs, oldest first.
    pub context: Vec<u32>,
    /// Source-model argmax token.
    pub target: u32,
    /// Source-model top candidates, sorted by score then token ID.
    pub emissions: Vec<ObservedEmission>,
}

/// Provenance recorded by the observation capture stage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservationMetadata {
    /// Hugging Face repository or local model identifier.
    pub model: String,
    /// Immutable model revision.
    pub revision: String,
    /// SHA-256 over the captured source tree and capture manifest.
    pub source_sha256: [u8; 32],
    /// Maximum context length used during capture.
    pub max_context: usize,
    /// Number of source candidates retained per observation.
    pub top_k: usize,
    /// SHA-256 over the tokenizer files used for capture.
    pub tokenizer_sha256: [u8; 32],
    /// SHA-256 over the canonical chat-template value, or the explicit no-template value.
    pub chat_template_sha256: [u8; 32],
    /// SHA-256 over the canonical special-token map.
    pub special_tokens_sha256: [u8; 32],
    /// EOS token ID used by the tokenizer/model identity.
    pub eos_token: u32,
}

/// Parsed observation corpus.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservationCorpus {
    /// Capture metadata.
    pub metadata: ObservationMetadata,
    /// Captured observations.
    pub observations: Vec<Observation>,
}

/// Failure to parse a captured observation corpus.
#[derive(Debug)]
pub enum ObservationError {
    /// Reading the corpus failed.
    Io(std::io::Error),
    /// The file header or one record is malformed.
    Malformed {
        /// One-based source line, or zero for whole-file errors.
        line: usize,
        /// Stable human-readable explanation.
        message: String,
    },
}

impl fmt::Display for ObservationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "reading observation corpus: {error}"),
            Self::Malformed { line, message } if *line == 0 => {
                write!(formatter, "malformed observation corpus: {message}")
            }
            Self::Malformed { line, message } => {
                write!(
                    formatter,
                    "malformed observation corpus at line {line}: {message}"
                )
            }
        }
    }
}

impl std::error::Error for ObservationError {}

impl From<std::io::Error> for ObservationError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl ObservationCorpus {
    /// Reads and parses a `UOROBS1` corpus.
    pub fn read(path: &Path) -> Result<Self, ObservationError> {
        let text = std::fs::read_to_string(path)?;
        Self::parse(&text)
    }

    /// Parses the line-oriented deterministic capture format.
    pub fn parse(text: &str) -> Result<Self, ObservationError> {
        let mut lines = text.lines().enumerate();
        let Some((_, first)) = lines.next() else {
            return Err(malformed(0, "file is empty"));
        };
        if first.trim() != "UOROBS1" {
            return Err(malformed(1, "first line must be UOROBS1"));
        }

        let mut model = None;
        let mut revision = None;
        let mut source_sha256 = None;
        let mut max_context = None;
        let mut top_k = None;
        let mut tokenizer_sha256 = None;
        let mut chat_template_sha256 = None;
        let mut special_tokens_sha256 = None;
        let mut eos_token = None;
        let mut observations = Vec::new();
        let mut in_records = false;

        for (zero_line, raw) in lines {
            let line_number = zero_line + 1;
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line == "--" {
                in_records = true;
                continue;
            }
            if !in_records {
                let Some((key, value)) = line.split_once('=') else {
                    return Err(malformed(line_number, "metadata must use key=value"));
                };
                match key {
                    "model" => model = Some(value.to_owned()),
                    "revision" => revision = Some(value.to_owned()),
                    "source_sha256" => source_sha256 = Some(parse_hex_32(value, line_number)?),
                    "max_context" => max_context = Some(parse_usize(value, line_number)?),
                    "top_k" => top_k = Some(parse_usize(value, line_number)?),
                    "tokenizer_sha256" => {
                        tokenizer_sha256 = Some(parse_hex_32(value, line_number)?)
                    }
                    "chat_template_sha256" => {
                        chat_template_sha256 = Some(parse_hex_32(value, line_number)?)
                    }
                    "special_tokens_sha256" => {
                        special_tokens_sha256 = Some(parse_hex_32(value, line_number)?)
                    }
                    "eos_token" => eos_token = Some(parse_u32(value, line_number)?),
                    _ => return Err(malformed(line_number, "unknown metadata key")),
                }
                continue;
            }
            observations.push(parse_observation(line, line_number)?);
        }

        let metadata = ObservationMetadata {
            model: model.ok_or_else(|| malformed(0, "model metadata is missing"))?,
            revision: revision.ok_or_else(|| malformed(0, "revision metadata is missing"))?,
            source_sha256: source_sha256
                .ok_or_else(|| malformed(0, "source_sha256 metadata is missing"))?,
            max_context: max_context
                .ok_or_else(|| malformed(0, "max_context metadata is missing"))?,
            top_k: top_k.ok_or_else(|| malformed(0, "top_k metadata is missing"))?,
            tokenizer_sha256: tokenizer_sha256
                .ok_or_else(|| malformed(0, "tokenizer_sha256 metadata is missing"))?,
            chat_template_sha256: chat_template_sha256
                .ok_or_else(|| malformed(0, "chat_template_sha256 metadata is missing"))?,
            special_tokens_sha256: special_tokens_sha256
                .ok_or_else(|| malformed(0, "special_tokens_sha256 metadata is missing"))?,
            eos_token: eos_token.ok_or_else(|| malformed(0, "eos_token metadata is missing"))?,
        };
        if metadata.max_context == 0 || metadata.max_context > uor_semantic::MAX_CONTEXT_TOKENS {
            return Err(malformed(0, "max_context exceeds the artifact format"));
        }
        if metadata.top_k == 0 || metadata.top_k > usize::from(u16::MAX) {
            return Err(malformed(0, "top_k is outside the supported range"));
        }
        if observations.is_empty() {
            return Err(malformed(0, "at least one observation is required"));
        }
        for observation in &observations {
            if observation.context.len() > metadata.max_context {
                return Err(malformed(0, "an observation exceeds max_context"));
            }
            if observation.emissions.len() > metadata.top_k {
                return Err(malformed(0, "an observation exceeds top_k"));
            }
        }

        Ok(Self {
            metadata,
            observations,
        })
    }
}

fn parse_observation(line: &str, line_number: usize) -> Result<Observation, ObservationError> {
    let mut fields = line.split('|');
    if fields.next() != Some("O") {
        return Err(malformed(line_number, "record must start with O|"));
    }
    let context_text = fields
        .next()
        .ok_or_else(|| malformed(line_number, "context field is missing"))?;
    let target_text = fields
        .next()
        .ok_or_else(|| malformed(line_number, "target field is missing"))?;
    let emissions_text = fields
        .next()
        .ok_or_else(|| malformed(line_number, "emission field is missing"))?;
    if fields.next().is_some() {
        return Err(malformed(line_number, "record contains extra fields"));
    }

    let mut context = Vec::new();
    if !context_text.is_empty() {
        for token in context_text.split(',') {
            context.push(parse_u32(token, line_number)?);
        }
    }
    if context.is_empty() {
        return Err(malformed(
            line_number,
            "context must contain at least one token",
        ));
    }

    let target = parse_u32(target_text, line_number)?;
    let mut emissions = Vec::new();
    let mut seen = BTreeSet::new();
    for item in emissions_text.split(',') {
        let Some((token_text, score_text)) = item.split_once(':') else {
            return Err(malformed(line_number, "emission must use token:score"));
        };
        let token = parse_u32(token_text, line_number)?;
        let score = score_text
            .parse::<i32>()
            .map_err(|_| malformed(line_number, "emission score is not an i32"))?;
        if !seen.insert(token) {
            return Err(malformed(line_number, "emission token is duplicated"));
        }
        emissions.push(ObservedEmission { token, score });
    }
    if emissions.is_empty() {
        return Err(malformed(line_number, "at least one emission is required"));
    }
    emissions.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.token.cmp(&right.token))
    });
    if emissions[0].token != target {
        return Err(malformed(
            line_number,
            "target must equal the canonical top-scoring emission",
        ));
    }

    Ok(Observation {
        context,
        target,
        emissions,
    })
}

fn parse_u32(text: &str, line_number: usize) -> Result<u32, ObservationError> {
    text.parse::<u32>()
        .map_err(|_| malformed(line_number, "token is not a u32"))
}

fn parse_usize(text: &str, line_number: usize) -> Result<usize, ObservationError> {
    text.parse::<usize>()
        .map_err(|_| malformed(line_number, "metadata value is not a usize"))
}

fn parse_hex_32(text: &str, line_number: usize) -> Result<[u8; 32], ObservationError> {
    if text.len() != 64 {
        return Err(malformed(
            line_number,
            "source_sha256 must contain 64 hex digits",
        ));
    }
    let bytes = text.as_bytes();
    let mut output = [0u8; 32];
    for (index, slot) in output.iter_mut().enumerate() {
        let high = decode_hex(bytes[index * 2])
            .ok_or_else(|| malformed(line_number, "source_sha256 contains non-hex data"))?;
        let low = decode_hex(bytes[index * 2 + 1])
            .ok_or_else(|| malformed(line_number, "source_sha256 contains non-hex data"))?;
        *slot = (high << 4) | low;
    }
    Ok(output)
}

fn decode_hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn malformed(line: usize, message: &str) -> ObservationError {
    ObservationError::Malformed {
        line,
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::ObservationCorpus;

    const CORPUS: &str = concat!(
        "UOROBS1\n",
        "model=fixture/model\n",
        "revision=0123456789abcdef\n",
        "source_sha256=0000000000000000000000000000000000000000000000000000000000000001\n",
        "max_context=4\n",
        "top_k=2\n",
        "tokenizer_sha256=0000000000000000000000000000000000000000000000000000000000000001\n",
        "chat_template_sha256=0000000000000000000000000000000000000000000000000000000000000002\n",
        "special_tokens_sha256=0000000000000000000000000000000000000000000000000000000000000003\n",
        "eos_token=2\n",
        "--\n",
        "O|1,2|3|3:100,4:90\n",
    );

    #[test]
    fn parses_canonical_fixture() {
        let corpus = ObservationCorpus::parse(CORPUS).expect("fixture parses");
        assert_eq!(corpus.observations.len(), 1);
        assert_eq!(corpus.observations[0].target, 3);
    }
}
