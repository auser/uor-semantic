Feature: Allocation-free generation behavior

  @GN-01 @build
  Scenario: Greedy generation follows exact compiled contexts
    Given two consecutive teacher context records
    When generation begins from the first context
    Then the two captured teacher argmax tokens are emitted

  @GN-02 @build
  Scenario: Autoregressive teacher rollouts include EOS parity
    Given a bounded prompt and an EOS-aware teacher rollout
    When rollout parity is evaluated
    Then sequence exact-match and EOS-position agreement are reported separately
