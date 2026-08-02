Feature: Source-model parity certification

  @GP-01 @build
  Scenario: Exact construction contexts match teacher argmax
    Given captured top-token teacher evidence
    When exact-context parity is evaluated
    Then every construction fixture prediction matches the teacher

  @GP-02 @build
  Scenario: Unmet parity thresholds cannot be certified
    Given measured exact and graph parity values
    When a threshold exceeds a measured value
    Then certification is rejected

  @GP-03 @build
  Scenario: Graph-only parity reports top-K recall
    Given captured teacher emissions with more than one candidate
    When graph-only parity is evaluated
    Then the report includes top-K recall and indexed candidate work

  @GP-04 @build
  Scenario: Graph certification enforces coverage-aware floors
    Given graph-only coverage and top-K recall measurements
    When certification requests coverage-aware thresholds
    Then a low-coverage artifact cannot pass on conditional recall alone

  @GP-05 @build
  Scenario: Accuracy profile selects cross-validated graph settings
    Given the compiler accuracy profile
    When its bounded graph calibration settings are inspected
    Then it uses the cross-validated region and emission limits

  @GP-06 @build
  Scenario: Accuracy profile retains bounded graph artifact work
    Given the default accuracy profile compiles a graph artifact
    When its artifact shape and graph work are inspected
    Then region emissions and indexed work remain bounded
