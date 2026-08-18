# Overview

A **declaration** is a source form that introduces a named program entity, a member, a module contribution, a
compile-time relation, or a semantic relationship.

Declarations are checked in a declaration context.

The declaration context determines:

- which declaration forms are accepted,
- which names are introduced,
- whether visibility modifiers are valid,
- whether directives can attach,
- whether a body is required or forbidden,
- which contracts and constraints are part of the declaration surface.

Declarations are not runtime statements.

A declaration can contain a body that is evaluated later, such as a function body, method body, constructor body,
finalizer body, or defaulted trait member body.

Declaring such a body does not evaluate the body.

## Navigation

- [Language index](../index.md)
- [Declarations index](../declarations.md)
- Next: [Declaration contexts](declaration-contexts.md)
