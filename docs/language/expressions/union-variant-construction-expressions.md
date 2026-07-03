# Union variant construction expressions

A **union variant construction expression** creates a fully initialized value of a union type with one active variant.

A full payload variant construction expression names the union type and variant.

```bray
let shape = Shape.Circle(center = origin, radius = 10.0);
```

An expected-type payload variant construction expression uses leading-dot shorthand when the expected union type is known.

```bray
let shape: Shape = .Circle(center = origin, radius = 10.0);
```

A full no-payload variant construction expression names the union type and variant.

```bray
let result = ParseResult<i32>.EndOfInput;
```

An expected-type no-payload variant construction expression uses leading-dot shorthand when the expected union type is known.

```bray
let result: ParseResult<i32> = .EndOfInput;
```

A leading-dot variant construction expression is valid when expression context provides a known union type and that union contains the named variant.

The full union path is required when the expected union type is absent or insufficient for resolution.

A no-payload variant construction expression uses no parentheses.

A payload variant construction expression uses parentheses containing payload field initializers.

Payload field initializers are comma-separated.

Trailing commas are allowed.

Named payload field initializers use `=`.

```bray
Shape.Circle(
    center = origin,
    radius = 10.0,
)
```

Payload fields are named by default.

Payload field names can be omitted only when the initializer supplies a `pos` payload field by position.

Named payload field order does not matter.

Each positional payload initializer supplies the corresponding `pos` payload field by declaration order.

Positional payload initializers must appear before named payload initializers.

A positional payload initializer for a non-`pos` payload field is rejected.

A payload field cannot be supplied both positionally and by name.

A variant construction expression that initializes the same payload field more than once is rejected.

A variant construction expression that names a payload field not declared by the selected variant is rejected.

Every payload field without a default must be initialized by the construction expression.

A payload field with a default can be omitted.

An omitted defaulted payload field is initialized from its declared default expression.

Variant payload default declaration rules are defined in [Union Types](../types/union-types.md#variant-payload-defaults).

A variant payload default is evaluated when the payload field is omitted during construction.

A supplied payload field initializer suppresses evaluation of that payload field’s default expression.

A payload field initializer expression is checked against the declared payload field type.

The declared payload field type can provide expected type context to the initializer expression.

Expected payload field type can guide literal typing, union variant shorthand, nested struct construction shorthand, box construction shorthand, tuple element typing, array element typing, and conversion checking.

```bray
let event: Event = .Nested(
    inner = .Started(time = now),
);
```

Variant contracts are checked during construction.

A `requires(...)` clause on a variant must be satisfied by the construction expression.

A successful variant construction can establish facts declared by the variant’s `ensures(...)` clause.

A successful variant construction establishes that the produced union value has the selected active variant.

For a payload variant, successful construction establishes that the selected payload exists and that its initialized payload fields are initialized.

For a no-payload variant, successful construction establishes the active variant and introduces no payload fields.

A union variant construction expression is fully initialized when the active tag has been initialized and the selected variant payload, if any, has been fully initialized.

Inactive variant payloads have no initialized values.

If evaluation exits before construction completes, already-initialized payload values and temporaries are handled by the corresponding control-flow, ownership, destruction, and finalization rules.

A union variant construction expression produces an owned value of the union type.

Each supplied payload initializer value is moved into its payload field unless the value is copied according to its type’s copy contract or another explicit rule applies.

Each defaulted payload value is moved into its payload field unless the default expression produces a copied value or another explicit rule applies.

Effects of supplied payload initializer expressions are effects of the union variant construction expression.

Effects of evaluated payload defaults are effects of the union variant construction expression.

Finalization obligations created by supplied payload initializer expressions or evaluated payload defaults become obligations of the constructed union value, local temporaries, or surrounding context according to ownership and lifecycle rules.

A union variant construction expression participates in capability checking.

A payload initializer can use only the capabilities available in the construction expression’s surrounding context.

A payload default can use only the capabilities available to the declaration that defines the default and to the construction context according to the default-expression rules.

Trusted capabilities used by defaults, payload initializers, or variant contracts must be permitted by the surrounding trusted declaration or rejected according to the Contract and Trust Model.

A union variant construction expression can establish facts in the fact context.

Facts can include the union type, selected active variant, initialized active payload, initialized payload fields, and facts established by payload initializer expressions, defaults, or variant contracts.

Mutation, movement, consumption, destruction, reinitialization, finalization, or capability loss can invalidate facts about the constructed union value or active payload.

A union variant construction expression creates a new value. Initialization performed by the construction expression is initialization, not ordinary mutation.

A value being constructed has no stable observable identity until construction is complete.

Payload field mutability controls post-initialization mutation of payload fields. It does not restrict initialization of payload fields during construction.

Supplied payload field initializer expressions are evaluated in source order.

Omitted payload field defaults are evaluated after supplied payload field initializers, in payload field declaration order.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Struct construction expressions](struct-construction-expressions.md)
- Next: [Box construction expressions](box-construction-expressions.md)
