# Summary

Contracts describe caller obligations, static constraints, and normal-completion guarantees.

Predicate expressions are restricted contract-level expressions.

Ordinary requirements are checkable predicate conditions.

Trusted requirements are trusted predicate conditions that must be established, preserved, or acknowledged.

Trusted implementation capabilities are implementation authority declared with `uses(...)`.

Trusted implementation capabilities do not automatically create trusted caller obligations.

Trusted caller obligations appear in the declaration contract.

Trust boundaries visibly acknowledge trusted caller obligations for a specific operand expression.

Trusted witness values carry trusted guarantees as part of ordinary value contracts.

Contract reasonings are flow-sensitive and are invalidated by ownership, mutation, lifetime, capability, and storage-state changes.

Trusted obligations used inside wrappers must be discharged or exposed.

Lifecycle and foreign boundaries follow the same contract and trust rules as ordinary callables.

## Navigation

- [Language index](../index.md)
- [Contracts and trust index](../contracts-and-trust.md)
- Previous: [Lifecycle and foreign boundaries](lifecycle-and-foreign-boundaries.md)
