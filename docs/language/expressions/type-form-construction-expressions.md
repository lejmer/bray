# Type-form construction expressions

A **type-form construction expression** is a construction expression associated with a type form.

A type form is a compiler-recognized type-level construct.

A type form can define construction behavior when constructing values of that type form requires compiler-recognized semantics.

`box` is the sole type form with construction expression syntax.

```bray
box[S] T
box[S](value, ...)
```

The type form determines the produced type.

The type form determines the subject type or subject types.

The type form determines any compile-time arguments.

The type form determines the relationship between the constructed value and the subject value.

The type form can affect ownership, storage, borrowing, layout, lifetime behavior, initialization, destruction, finalization, access-path projection, effect checking, and capability checking.

The square-bracket part of a type-form construction expression contains compile-time arguments for the type form.

```bray
box[Heap](value)
```

The parentheses contain runtime construction arguments.

Runtime arguments follow the call surface defined by the type-form construction behavior.

An argument supplied in named runtime-parameter form writes the runtime parameter name explicitly.

A type-form construction expression can use expected type context.

Expected type context can supply the type form, compile-time arguments, subject type, storage policy type, or other type-form-specific expected information.

Expected subject type can propagate inward to the subject value expression when the type form defines a single clear subject type.

A type-form construction expression is not an ordinary function call.

A type-form construction expression is checked by the compiler according to the construction behavior defined for that type form.

The construction behavior can invoke ordinary declarations, trait behavior, lifecycle declarations, storage behavior, trusted declarations, and contract clauses, but the type-form construction expression itself remains a compiler-recognized expression form.

Only type forms with defined construction behavior have type-form construction expression syntax.

A type form with no construction behavior cannot be used as a construction expression merely because it has type syntax.

A type-form construction expression produces a fully initialized value when all construction steps required by the type form have completed.

If evaluation exits before construction completes, already-initialized values, partially initialized storage, temporaries, and acquired capabilities are handled by the corresponding control-flow, ownership, destruction, finalization, and capability rules.

A type-form construction expression participates in type checking, ownership checking, initialization checking, destruction checking, finalization tracking, effect checking, capability checking, trusted capability checking, and fact-context refinement.

A type-form construction expression can establish facts in the fact context according to the construction behavior of the type form.

Facts established by a type-form construction expression remain valid only while the values, storage identities, lifetimes, capabilities, and versions they depend on remain valid.

Runtime construction arguments are evaluated in source order.

Omitted runtime construction defaults are evaluated after explicit runtime construction arguments, in construction parameter declaration order.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Box construction expressions](box-construction-expressions.md)
- Next: [Match expressions](match-expressions.md)
