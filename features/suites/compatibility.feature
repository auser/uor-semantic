Feature: R4G1 and TLA5 compatibility bridge

  @CX-01 @build
  Scenario: Compatibility manifests bind target identities to a semantic artifact
    Given a compiled semantic artifact and a target artifact format
    When a typed R4G1 or TLA5 compatibility manifest is created
    Then its artifact, source, and tokenizer identities validate before use

  @CX-02 @build
  Scenario: Compatibility status mapping preserves target resolution outcomes
    Given exact-context, graph, novel, and contradictory target statuses
    When statuses are mapped to the semantic prediction model
    Then supported outcomes remain distinct and contradictory is not silently served

  @CX-03 @build
  Scenario: Compatibility witnesses carry replay-critical evidence
    Given a status-aware semantic prediction
    When a compatibility witness is validated
    Then identity, selected token, status, and graph-region requirements are enforced

  @CX-04 @build
  Scenario: Compatibility prediction equivalence preserves ranked output
    Given a semantic prediction and a target-compatible candidate list
    When prediction equivalence is checked
    Then token-score order, status, and widening state must match exactly

  @CX-05 @build
  Scenario: R4G1 identity headers map into compatibility manifests
    Given a bounded R4G1 container with a valid HEAD section
    When its fixed header and HEAD identities are adapted
    Then the artifact, teacher, and tokenizer identities bind to the semantic manifest

  @CX-06 @build
  Scenario: Invalid R4G1 identity headers are rejected before identity exposure
    Given a truncated, unsupported, mis-sized, unaligned, or head-less R4G1 container
    When the bounded identity adapter parses it
    Then it returns a typed structural error without exposing a manifest

  @CX-07 @build
  Scenario: R4G1 structure views expose required sections without allocation
    Given a canonically ordered R4G1 container with every required section
    When a borrowed structure view is created
    Then required section payloads remain addressable through borrowed slices

  @CX-08 @build
  Scenario: R4G1 structure validation rejects unsafe section topology
    Given an R4G1 container with an unknown mandatory section, missing required section, or overlap
    When its borrowed structure view is created
    Then the structural error is returned before section access

  @CX-09 @build
  Scenario: R4G1 graph views validate bounded record sections
    Given a structurally valid R4G1 graph with bounded HEAD declarations
    When its borrowed graph view is created
    Then NODE and EDGE records are exposed only after exact section validation

  @CX-10 @build
  Scenario: R4G1 graph views reject invalid ranges and endpoints
    Given a R4G1 graph with an invalid node range, depth, ROUT window, or edge endpoint
    When its borrowed graph view is created
    Then it returns a typed semantic validation error

  @CX-11 @build
  Scenario: Typed lifecycle entry points download and compile
    Given typed requests for an immutable source and an observation corpus
    When the download and compile entry points are called
    Then the download request is pinned and the compile request writes a validated artifact

  @CX-12 @build
  Scenario: Typed source compilation preflights before the teacher bridge
    Given a local model snapshot and a missing construction corpus
    When typed source compilation is requested
    Then it returns a typed corpus failure before invoking Python

  @CX-13 @build
  Scenario: R4G1 graph views validate ROUT bytecode
    Given a graph with an unknown ROUT opcode, invalid operand, jump, or shortlist
    When its borrowed graph view is created
    Then it returns a typed ROUT validation error

  @CX-14 @build
  Scenario: Compiled artifacts export to canonical structural R4G1
    Given a validated compiled semantic artifact
    When it is exported through the R4G1 compatibility bridge
    Then the borrowed graph view accepts its sections and both BLAKE3 CIDs verify
