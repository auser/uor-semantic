Feature: Divisible semantic paths

  @SP-01 @build
  Scenario: Every initialized prefix remains meaningful
    Given the four-slot path 17 42 8 3
    When every prefix depth from zero through four is requested
    Then each prefix returns the corresponding leading slot sequence

  @SP-02 @build
  Scenario: Compiled paths follow learned prototype hierarchy
    Given three calibrated prototypes with nearest-neighbor relationships
    When compiled region paths are derived
    Then each path extends its nearest eligible parent with a deterministic child slot
