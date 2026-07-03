# Lifecycle requirements in traits

A lifecycle requirement is a trait member that requires compatible lifecycle behavior from the implementing subject or exact trait implementation.

Trait lifecycle requirement syntax is defined in [Traits](../types/traits.md#lifecycle-requirements-in-traits).

The lifecycle requirements allowed in traits are:

- `finalize`,
- `destruct`,
- `enter`,
- `exit`.

Constructor requirements are expressed as static callable members that return `Self` or `Result<Self, E>`.

A `finalize` requirement is satisfied by compatible finalization behavior on the implementing subject.

A `destruct` requirement is satisfied by compatible destruction behavior on the implementing subject.

Trait implementations cannot provide `finalize` or `destruct` bodies.

`finalize` and `destruct` remain lifecycle behavior of the concrete subject.

Finalization and destruction do not depend on which trait view or trait implementation is used to observe a value.

Multiple traits can require `finalize` or `destruct` from the same implementing subject.

The implementing subject's lifecycle behavior must satisfy every participating `finalize` or `destruct` requirement.

A `destruct` requirement must be synchronous, infallible, and return `unit`.

If the result type is omitted from a `destruct` requirement, `unit` is inferred.

A `finalize` requirement can be synchronous or asynchronous.

A `finalize` requirement can return `unit` or `Result<unit, E>`.

The implementing subject's finalization behavior must match the requirement's execution mode and result shape.

For a `Result<unit, E>` requirement, the finalizer error type must be compatible with `E`.

The type-wide finalizer must satisfy the requirement's contract clauses.

An `enter` requirement must be paired with a matching `exit` requirement in the same trait.

An `enter` or `exit` requirement can also be satisfied by a compatible lifecycle declaration in the trait implementation body.

An implementation lifecycle declaration can fulfill only an `enter` or `exit` requirement declared by the implemented trait.

An implementation cannot provide lifecycle declarations that do not fulfill lifecycle requirements declared by the trait.

## Navigation

- [Language index](../index.md)
- [Lifecycle index](../lifecycle.md)
- Previous: [Scope exits, panics, and cancellation](scope-exits-panics-and-cancellation.md)
- Next: [API compatibility](api-compatibility.md)
