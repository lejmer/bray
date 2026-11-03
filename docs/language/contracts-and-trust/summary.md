# Summary

This summary is informative. The detailed rules include:

- [contract clauses](contract-clauses.md).
- [contract reasoning](contract-reasoning.md).
- [trusted caller obligations](trusted-caller-obligations.md).

Contracts describe caller obligations, static constraints, and normal-completion guarantees. Predicate expressions are
restricted contract-level expressions. Ordinary requirements are checkable conditions, while trusted requirements must
be established, preserved, or acknowledged through the trust rules.

Trusted implementation capabilities are declared with `uses(...)` and do not automatically create caller obligations.
Trusted caller obligations appear in the declaration contract, and trust boundaries visibly acknowledge them for a
specific operand. Trusted witness values carry guarantees as part of ordinary value contracts.

Contract reasoning follows control flow. Ownership, mutation, lifetime, capability, and storage-state changes can
invalidate its evidence. Wrappers must discharge or expose the trusted obligations they use, and lifecycle and foreign
boundaries follow the same contract and trust rules as ordinary callables.

## Navigation

- [Language index](../index.md)
- [Contracts and trust index](../contracts-and-trust.md)
- Previous: [Lifecycle and foreign boundaries](lifecycle-and-foreign-boundaries.md)
