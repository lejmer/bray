# Axiom 8: Abstraction is behavioral

Abstraction is expressed through behavioral contracts.

A behavioral contract declares operations, associated types, capability requirements, effect requirements, and semantic obligations that an implementing type must provide.

A type's identity comes from its own declaration. A type gains abstract behavior through explicit implementations of behavioral contracts.

Implementing a behavioral contract creates a declared relationship between the type and the contract. That relationship is part of the program's semantic surface and
participates in type checking, generic constraints, dispatch, documentation, and public API compatibility.

Generic code abstracts over behavior by declaring the contracts it requires. A generic body may use only the behavior guaranteed by its declared contracts.

Dynamic abstraction uses explicit contract-bearing values. A value whose behavior is selected dynamically carries that abstraction model in its type or public contract.

Shared implementation is expressed through composition, delegation, default contract methods, or explicitly imported helper behavior. Reuse of implementation does not create
inherited type identity.

Behavioral contracts may impose capability and effect requirements. A contract can require observation, mutation authority, consumption, internal effects, trusted capabilities,
or other declared operations when those requirements are part of the behavior being abstracted.

The compiler resolves behavioral abstraction through explicit declarations and deterministic lookup rules.
