Feature: Strict CPU-native runtime boundary

  @RT-01 @build
  Scenario: The shipped core preserves its strict source contract
    Given the published uor-semantic crate
    When its manifest and production Rust source are audited
    Then no dependency, heap type, unsafe escape, float, product, quotient, or remainder operator is present

  @RT-02 @build
  Scenario: Warm routing performs no heap operation
    Given a validated fixture and caller-owned routing storage
    When 1024 warmed route calls execute under a counting allocator
    Then observed allocations and deallocations remain unchanged

  @RT-03 @build
  Scenario: The complete deployed path remains heap-free
    Given a validated artifact and warmed caller-owned buffers
    When parsing, prediction, and greedy generation repeat
    Then allocation and deallocation counts remain unchanged
