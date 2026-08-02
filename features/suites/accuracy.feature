Feature: Contextual fixture accuracy

  @AC-01 @build
  Scenario: Context disambiguates the pinned meanings of stop
    Given context-free stop, stop the car, and wait at the bus stop fixtures
    When each fixture is routed without exact-context lookup
    Then every case has the declared highest-ranked region and active count

  @AC-02 @build
  Scenario: Held-out graph accuracy is measured without exact context
    Given a held-out context absent from the exact-context section
    When graph-only parity is evaluated
    Then the report records graph coverage and teacher argmax agreement

  @AC-03 @build
  Scenario: Separate held-out inputs cannot use exact lookup
    Given separate construction and held-out observation inputs
    When the held-out parity lane is evaluated
    Then exact coverage remains zero even when a context overlaps construction

  @AC-04 @build
  Scenario: Graph-only parity reports indexed candidate work
    Given a compiled artifact with a serialized candidate index
    When graph-only parity is evaluated
    Then the report includes the indexed regions scanned across its samples

  @AC-05 @build
  Scenario: Symmetric graph cross-validation disables exact lookup
    Given two separate construction and held-out observation corpora
    When each corpus is used in both calibration directions
    Then both graph-only reports have zero exact coverage
