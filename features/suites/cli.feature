Feature: End-to-end command-line testing

  @CL-01 @build
  Scenario: The self-test covers the complete local pipeline
    Given the embedded deterministic observation fixture
    When the CLI self-test runs
    Then compilation, runtime validation, parity, and generation pass

  @CL-02 @build
  Scenario: Interactive artifact commands exercise a compiled artifact
    Given a compiled semantic artifact
    When inspect, predict, and bounded generate commands run
    Then every interactive artifact command succeeds
