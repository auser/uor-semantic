Feature: Deterministic semantic compilation

  @CP-01 @build
  Scenario: Canonical compilation is reproducible
    Given the same observation corpus and compiler profile
    When compilation is executed twice
    Then both artifact bytes and codebook identities are equal

  @CP-02 @build
  Scenario: Existing model work is protected
    Given an unverified existing model source directory
    When model build preflight runs
    Then it refuses to overwrite the existing work
