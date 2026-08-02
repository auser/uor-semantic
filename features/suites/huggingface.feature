Feature: Reproducible Hugging Face model ingestion

  @HF-01 @build
  Scenario: Model downloads are revision pinned
    Given a Hugging Face repository and immutable commit
    When the download command is constructed
    Then it uses hf download with revision and local-dir arguments

  @HF-02 @build
  Scenario: Captured teacher observations compile
    Given a valid UOROBS1 capture from a Hugging Face causal language model
    When the observation compiler runs
    Then the strict runtime validates the emitted artifact

  @HF-04 @build
  Scenario: Model build validates local inputs before download
    Given a missing, empty, unreadable, or invalid model-build input
    When model build preflight runs
    Then it reports an actionable typed failure without invoking the downloader

  @HF-05 @build
  Scenario: Corpus paths are stable across child working directories
    Given a valid relative corpus path
    When model build preflight resolves the path
    Then the capture bridge receives an absolute corpus path

  @HF-06 @build
  Scenario: Tokenizer identity is bound to parity
    Given capture metadata for tokenizer files, chat template, special tokens, and EOS
    When parity evaluates a corpus with a mismatched tokenizer identity
    Then it fails before measuring model behavior
