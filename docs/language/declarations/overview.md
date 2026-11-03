# Overview

A **declaration** introduces a program entity, a member, or a semantic relationship. This includes module contributions
and compile-time relations.

The [declaration context](declaration-contexts.md) determines which declaration forms are accepted and which names they
introduce. It also determines whether visibility modifiers or directives are valid, whether a body is required or
forbidden, and which contracts and constraints belong to the declaration.

Declarations are not runtime statements. A declaration can contain a body evaluated later, such as a function, method,
constructor, finalizer, or defaulted trait member body. Declaring the body does not evaluate it.

## Navigation

- [Language index](../index.md)
- [Declarations index](../declarations.md)
- Next: [Declaration contexts](declaration-contexts.md)
