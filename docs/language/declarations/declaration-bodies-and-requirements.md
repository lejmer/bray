# Declaration bodies and requirements

A declaration body is the syntactic body attached to a declaration.

Whether a body is required, optional, or forbidden depends on the declaration form.

Function declarations with Bray implementations have callable-body block expressions.

Extern callable declarations have no Bray body and end with `;`.

Type declarations have type bodies.

Trait declarations have trait bodies.

Implementation declarations have implementation bodies.

Predicate declarations either have a predicate expression body or, for trusted opaque predicates, end with `;`.

Callable overload declarations and implementation overload declarations have overload arm lists and do not define executable bodies.

Named callable contract declarations have no executable body and end with `;`.

Required trait callable members end with `;`.

Defaulted trait callable members have callable-body block expressions.

Trait implementation callable members always have callable-body block expressions.

Trait type-valued member declarations end with `;`.

Trait implementation type-valued member bindings use `= type-expression;`.

Trait constant-valued member declarations can be required or defaulted according to the trait rules.

Trait implementation constant-valued member definitions always provide an initializer.

Declarations with bodies are checked in the context established by the declaration header.

Declaring a body does not evaluate that body.

Executable bodies run only when the corresponding callable, lifecycle operation, constructor, test entry, task, or entry point is invoked by the language rules.

## Navigation

- [Language index](../index.md)
- [Declarations index](../declarations.md)
- Previous: [Generic declarations and constraints](generic-declarations-and-constraints.md)
- Next: [Constant declarations](constant-declarations.md)
