Feature: Reproducible work and honest speed measurement

  @PF-01 @build
  Scenario: Routing work fits a declared deterministic budget
    Given three one-word candidates and an active capacity of three
    When the ambiguous fixture is routed repeatedly
    Then every operation census is identical and inside the declared bound

  @PF-02 @build
  Scenario: Empirical latency is reported with its measurement context
    Given a non-zero sample and iteration configuration
    When the route benchmark executes
    Then it reports architecture, sample dimensions, latency observations, and deterministic work counts
