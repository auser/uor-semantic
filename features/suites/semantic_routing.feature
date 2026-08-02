Feature: Bounded overlapping semantic routing

  @SR-01 @build
  Scenario: An ambiguous context retains multiple meanings
    Given the pinned stop-ambiguity codebook fixture
    When the context-free word stop is routed
    Then cessation, command, and transportation memberships are retained

  @SR-02 @build
  Scenario: Candidate order cannot change canonical output
    Given every permutation of the same candidate regions
    When the ambiguous context is routed through each permutation
    Then every route cloud has the same ordered memberships

  @SR-03 @build
  Scenario: Compiler calibration retains genuine overlapping memberships
    Given two prototype regions within the configured overlap margin
    When observation memberships are calibrated
    Then one observation may be retained by both prototype regions

  @RH-01 @build
  Scenario: Region boundaries are inclusive
    Given a context exactly one masked bit from a radius-one prototype
    When masked Hamming membership is evaluated
    Then the context is accepted with a zero boundary margin

  @RC-01 @build
  Scenario: Capacity pressure is explicit
    Given three accepted regions and room for two memberships
    When the ambiguous context is routed
    Then two canonical memberships are retained and one truncation is reported
