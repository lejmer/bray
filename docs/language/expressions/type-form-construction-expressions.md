# Type-form construction expressions

Syntax follows the [type form construction expressions grammar](../syntax-grammar.md#type-form-construction-expressions).

A **type-form construction expression** is a construction expression associated with a type form.

A type form is a compiler-recognized type-level construct.

The `box` type form defines construction behavior because constructing owned indirection requires compiler-recognized
semantics.

`box` is the sole type form with construction expression syntax.

```bray
box[S] T
box[S](value, ...)
```

The type form determines the produced type, subject types, compile-time arguments, and relationship between the
constructed value and its subject. For `box`, that relationship includes storage ownership and access to the contained
value. The selected form's [type rules](../types/type-forms.md) govern layout, borrowing, lifetime dependencies,
initialization, destruction, and finalization.

The square-bracket part of a type-form construction expression contains compile-time arguments for the type form.

```bray
box[Heap](value)
```

The parentheses contain runtime construction arguments.

Runtime arguments follow the call surface defined by the type-form construction behavior.

An argument supplied in named runtime-parameter form writes the runtime parameter name explicitly.

A type-form construction expression can use expected type context.

Expected type context can supply the type form, compile-time arguments, subject type, storage policy type, or other
type-form-specific expected information.

Expected subject type can propagate inward to the subject value expression when the type form defines a single clear
subject type.

A type-form construction expression is not an ordinary function call.

A type-form construction expression is checked by the compiler according to the construction behavior defined for that
type form.

The construction behavior can invoke ordinary declarations, trait behavior, lifecycle declarations, storage behavior,
trusted declarations, and contract clauses, but the type-form construction expression itself remains a
compiler-recognized expression form.

No other type form has type-form construction expression syntax. Type syntax alone never creates construction syntax.

A type-form construction expression produces a fully initialized value when all construction steps required by the type
form have completed.

If evaluation exits before construction completes, already-initialized values, partially initialized storage,
temporaries, and acquired capabilities are handled by the corresponding control-flow, ownership, destruction,
finalization, and capability rules.

The compiler checks each required construction operation for type, ownership, initialization, effects, and capabilities,
including trusted capabilities. Values and storage created by those operations retain their destruction and
finalization obligations under the selected construction contract.

A type-form construction expression can establish conditions at that program point according to the construction
behavior of the type form.

Conditions established by a type-form construction expression remain valid only while the values, storage identities,
lifetimes, capabilities, and versions they depend on remain valid.

Runtime construction arguments are evaluated in source order.

[Construction evaluation order](expression-evaluation-order.md#construction-and-scoped-use) defines the ordering of
explicit runtime arguments and omitted construction defaults.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Box construction expressions](box-construction-expressions.md)
- Next: [Match expressions](match-expressions.md)
