Feature: Repository claim integrity

  @CM-01 @build
  Scenario: Generated conformance documentation has one model source
    Given the model registers are parseable
    When CONFORMANCE.md is rendered in memory
    Then its bytes equal the committed document

  @CM-02 @build
  Scenario: Every capability has a scenario and a named Rust test
    Given all registered conformance IDs
    When feature suites and Rust test names are cross-checked
    Then every ID appears in all three inventories

  @CM-03 @build
  Scenario: Every cited authority is stable and findable
    Given the authority register
    When its rows are validated
    Then every citation has a revision marker or a stated checksum exception
