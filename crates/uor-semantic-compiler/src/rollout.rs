//! Parsing and bounded representation of autoregressive teacher rollouts.

use core::fmt;
use std::path::Path;

/// Maximum generated tokens retained for one rollout.
pub const MAX_ROLLOUT_TOKENS: usize = 128;

/// Provenance recorded by rollout capture.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RolloutMetadata {
    /// Hugging Face repository or local model identifier.
    pub model: String,
    /// Immutable model revision.
    pub revision: String,
    /// SHA-256 over the captured source tree and capture manifest.
    pub source_sha256: [u8; 32],
    /// Maximum prompt context retained for runtime generation.
    pub max_context: usize,
    /// Maximum generated tokens retained per rollout.
    pub max_tokens: usize,
    /// EOS token ID used to stop capture.
    pub eos_token: u32,
    /// SHA-256 over the tokenizer files used for capture.
    pub tokenizer_sha256: [u8; 32],
    /// SHA-256 over the canonical chat-template value, or the explicit no-template value.
    pub chat_template_sha256: [u8; 32],
    /// SHA-256 over the canonical special-token map.
    pub special_tokens_sha256: [u8; 32],
}

/// One autoregressive prompt and the teacher's generated token sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rollout {
    /// Prompt token IDs used to seed the bounded runtime context.
    pub prompt: Vec<u32>,
    /// Teacher-generated token IDs, including EOS when reached.
    pub generated: Vec<u32>,
    /// Zero-based EOS position, or no value when the budget ended first.
    pub eos_position: Option<usize>,
}

/// Parsed autoregressive rollout corpus.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RolloutCorpus {
    /// Capture metadata.
    pub metadata: RolloutMetadata,
    /// Captured rollout records.
    pub rollouts: Vec<Rollout>,
}

/// Failure to parse a rollout corpus.
#[derive(Debug)]
pub enum RolloutError {
    /// Reading the file failed.
    Io(std::io::Error),
    /// The header or a rollout record is malformed.
    Malformed {
        /// One-based source line, or zero for whole-file errors.
        line: usize,
        /// Stable human-readable explanation.
        message: String,
    },
}

impl fmt::Display for RolloutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "reading rollout corpus: {error}"),
            Self::Malformed { line, message } if *line == 0 => {
                write!(formatter, "malformed rollout corpus: {message}")
            }
            Self::Malformed { line, message } => {
                write!(
                    formatter,
                    "malformed rollout corpus at line {line}: {message}"
                )
            }
        }
    }
}

impl std::error::Error for RolloutError {}

impl From<std::io::Error> for RolloutError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl RolloutCorpus {
    /// Reads and parses a UORROL1 corpus.
    pub fn read(path: &Path) -> Result<Self, RolloutError> {
        let text = std::fs::read_to_string(path)?;
        Self::parse(&text)
    }

    /// Parses the line-oriented rollout format.
    pub fn parse(text: &str) -> Result<Self, RolloutError> {
        let mut lines = text.lines().enumerate();
        let Some((_, first)) = lines.next() else {
            return Err(malformed(0, "file is empty"));
        };
        if first.trim() != "UORROL1" {
            return Err(malformed(1, "first line must be UORROL1"));
        }

        let mut model = None;
        let mut revision = None;
        let mut source_sha256 = None;
        let mut max_context = None;
        let mut max_tokens = None;
        let mut eos_token = None;
        let mut tokenizer_sha256 = None;
        let mut chat_template_sha256 = None;
        let mut special_tokens_sha256 = None;
        let mut rollouts = Vec::new();
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
                    "max_tokens" => max_tokens = Some(parse_usize(value, line_number)?),
                    "eos_token" => eos_token = Some(parse_u32(value, line_number)?),
                    "tokenizer_sha256" => {
                        tokenizer_sha256 = Some(parse_hex_32(value, line_number)?)
                    }
                    "chat_template_sha256" => {
                        chat_template_sha256 = Some(parse_hex_32(value, line_number)?)
                    }
                    "special_tokens_sha256" => {
                        special_tokens_sha256 = Some(parse_hex_32(value, line_number)?)
                    }
                    _ => return Err(malformed(line_number, "unknown metadata key")),
                }
                continue;
            }
            rollouts.push(parse_rollout(
                line,
                line_number,
                max_context.unwrap_or(0),
                max_tokens.unwrap_or(0),
                eos_token.unwrap_or(0),
            )?);
        }

        let metadata = RolloutMetadata {
            model: model.ok_or_else(|| malformed(0, "model metadata is missing"))?,
            revision: revision.ok_or_else(|| malformed(0, "revision metadata is missing"))?,
            source_sha256: source_sha256
                .ok_or_else(|| malformed(0, "source_sha256 metadata is missing"))?,
            max_context: max_context
                .ok_or_else(|| malformed(0, "max_context metadata is missing"))?,
            max_tokens: max_tokens.ok_or_else(|| malformed(0, "max_tokens metadata is missing"))?,
            eos_token: eos_token.ok_or_else(|| malformed(0, "eos_token metadata is missing"))?,
            tokenizer_sha256: tokenizer_sha256
                .ok_or_else(|| malformed(0, "tokenizer_sha256 metadata is missing"))?,
            chat_template_sha256: chat_template_sha256
                .ok_or_else(|| malformed(0, "chat_template_sha256 metadata is missing"))?,
            special_tokens_sha256: special_tokens_sha256
                .ok_or_else(|| malformed(0, "special_tokens_sha256 metadata is missing"))?,
        };
        if metadata.max_context == 0 || metadata.max_context > uor_semantic::MAX_CONTEXT_TOKENS {
            return Err(malformed(0, "max_context exceeds the artifact format"));
        }
        if metadata.max_tokens == 0 || metadata.max_tokens > MAX_ROLLOUT_TOKENS {
            return Err(malformed(0, "max_tokens exceeds the rollout limit"));
        }
        if rollouts.is_empty() {
            return Err(malformed(0, "at least one rollout is required"));
        }
        for rollout in &rollouts {
            if rollout.prompt.is_empty() {
                return Err(malformed(0, "rollout prompt must contain a token"));
            }
            if rollout.prompt.len() > metadata.max_context {
                return Err(malformed(0, "rollout prompt exceeds max_context"));
            }
            if rollout.generated.is_empty() || rollout.generated.len() > metadata.max_tokens {
                return Err(malformed(0, "rollout exceeds max_tokens"));
            }
            if let Some(position) = rollout.eos_position
                && (position >= rollout.generated.len()
                    || rollout.generated[position] != metadata.eos_token)
            {
                return Err(malformed(0, "eos_position does not identify eos_token"));
            }
        }

        Ok(Self { metadata, rollouts })
    }
}

fn parse_rollout(
    line: &str,
    line_number: usize,
    max_context: usize,
    max_tokens: usize,
    eos_token: u32,
) -> Result<Rollout, RolloutError> {
    let mut fields = line.split('|');
    if fields.next() != Some("R") {
        return Err(malformed(line_number, "record must start with R|"));
    }
    let prompt = parse_tokens(
        fields
            .next()
            .ok_or_else(|| malformed(line_number, "prompt field is missing"))?,
        line_number,
    )?;
    let generated = parse_tokens(
        fields
            .next()
            .ok_or_else(|| malformed(line_number, "generated field is missing"))?,
        line_number,
    )?;
    let eos_text = fields
        .next()
        .ok_or_else(|| malformed(line_number, "eos position is missing"))?;
    if fields.next().is_some() {
        return Err(malformed(line_number, "record contains extra fields"));
    }
    if prompt.is_empty() || prompt.len() > max_context {
        return Err(malformed(line_number, "prompt is outside max_context"));
    }
    if generated.is_empty() || generated.len() > max_tokens {
        return Err(malformed(
            line_number,
            "generated sequence is outside max_tokens",
        ));
    }
    let eos_position = if eos_text == "-" {
        None
    } else {
        let position = eos_text
            .parse::<usize>()
            .map_err(|_| malformed(line_number, "eos position is not a usize"))?;
        if position >= generated.len() || generated[position] != eos_token {
            return Err(malformed(
                line_number,
                "eos position does not identify eos_token",
            ));
        }
        Some(position)
    };
    Ok(Rollout {
        prompt,
        generated,
        eos_position,
    })
}

fn parse_tokens(text: &str, line_number: usize) -> Result<Vec<u32>, RolloutError> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    text.split(',')
        .map(|token| parse_u32(token, line_number))
        .collect()
}

fn parse_u32(text: &str, line_number: usize) -> Result<u32, RolloutError> {
    text.parse::<u32>()
        .map_err(|_| malformed(line_number, "token is not a u32"))
}

fn parse_usize(text: &str, line_number: usize) -> Result<usize, RolloutError> {
    text.parse::<usize>()
        .map_err(|_| malformed(line_number, "metadata value is not a usize"))
}

fn parse_hex_32(text: &str, line_number: usize) -> Result<[u8; 32], RolloutError> {
    if text.len() != 64 {
        return Err(malformed(
            line_number,
            "source_sha256 must contain 64 hex digits",
        ));
    }
    let mut bytes = [0u8; 32];
    for (index, pair) in text.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_digit(pair[0]).ok_or_else(|| malformed(line_number, "invalid sha256"))?;
        let low = hex_digit(pair[1]).ok_or_else(|| malformed(line_number, "invalid sha256"))?;
        bytes[index] = (high << 4) | low;
    }
    Ok(bytes)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn malformed(line: usize, message: &str) -> RolloutError {
    RolloutError::Malformed {
        line,
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::RolloutCorpus;

    const CORPUS: &str = concat!(
        "UORROL1\n",
        "model=fixture/model\n",
        "revision=0123456789abcdef0123456789abcdef01234567\n",
        "source_sha256=0000000000000000000000000000000000000000000000000000000000000001\n",
        "max_context=4\n",
        "max_tokens=3\n",
        "eos_token=2\n",
        "tokenizer_sha256=0000000000000000000000000000000000000000000000000000000000000001\n",
        "chat_template_sha256=0000000000000000000000000000000000000000000000000000000000000002\n",
        "special_tokens_sha256=0000000000000000000000000000000000000000000000000000000000000003\n",
        "--\n",
        "R|1,2|3,2|1\n",
    );

    #[test]
    fn parses_eos_rollout() {
        let corpus = RolloutCorpus::parse(CORPUS).expect("rollout parses");
        assert_eq!(corpus.rollouts.len(), 1);
        assert_eq!(corpus.rollouts[0].eos_position, Some(1));
    }
}
