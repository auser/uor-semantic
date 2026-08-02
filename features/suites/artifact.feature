Feature: Compiled artifact compatibility

  @AR-01 @build
  Scenario: Compiler output is consumed by the strict runtime
    Given a deterministic captured-observation fixture
    When it is compiled and parsed through a borrowed artifact view
    Then the exact teacher prediction is preserved

  @AR-02 @build
  Scenario: Artifact identity is verified before runtime use
    Given a compiled artifact whose bytes have been modified
    When the zero-copy parser validates it
    Then it rejects the codebook identity mismatch

  @AR-03 @build
  Scenario: Artifact parser resource limits are bounded
    Given an artifact declaring an excessive section count
    When the zero-copy parser validates it
    Then it rejects the resource limit before record access

  @AR-04 @build
  Scenario: Graph prediction uses bounded candidate indexing
    Given an artifact with regions in distinct coarse signature buckets
    When graph-only prediction runs
    Then it scans fewer indexed candidates than the total region count
