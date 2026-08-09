# Struct construction expressions

A **struct construction expression** creates a fully initialized value of a struct type.

A full struct construction expression names the struct type before the construction body.

```bray
let p = Point
{
    x = 1.0,
    y = 2.0,
};
```

An expected-type struct construction expression omits the struct type when the expected type is known.

```bray
let p: Point =
{
    x = 1.0,
    y = 2.0,
};
```

The expected-type form is valid when expression context provides a known struct type.

The full type form is required when the expected type is absent or insufficient for resolution.

The construction body contains field initializers.

Field initializers are comma-separated.

Trailing commas are allowed.

Field initializers use `=`.

```bray
Point
{
    x = 1.0,
    y = 2.0,
}
```

Each field initializer names a field of the struct.

Field names cannot be omitted.

Field order does not matter.

A construction expression that initializes the same field more than once is rejected.

A construction expression that names a field not declared by the struct is rejected.

Every field without a default must be initialized by the construction expression.

A field with a default can be omitted.

An omitted defaulted field is initialized from its declared default expression.

Field default declaration rules are defined in [Product Types](../types/product-types.md#field-defaults).

A field default is evaluated when the field is omitted during construction.

A supplied field initializer suppresses evaluation of that field’s default expression.

A field initializer expression is checked against the declared field type.

The declared field type can provide expected type context to the initializer expression.

Expected field type can guide literal typing, union variant shorthand, nested struct construction shorthand, box construction shorthand, tuple element typing, array element typing, and conversion checking.

```bray
let shape: Shape =
{
    center = { x = 0.0, y = 0.0, },
    kind = Circle(radius = 1.0),
};
```

A struct construction expression is fully initialized when every required field has been initialized and every omitted defaulted field has been initialized from its default.

Each field has its own initialization state during construction.

If evaluation exits before construction completes, already-initialized field values and temporaries are handled by the corresponding control-flow, ownership, destruction, and finalization rules.

A struct construction expression produces an owned value of the constructed struct type.

Each supplied initializer value is moved into its field unless the value is copied according to its type’s copy contract or another explicit rule applies.

Each defaulted field value is moved into its field unless the default expression produces a copied value or another explicit rule applies.

Effects of supplied field initializer expressions are effects of the struct construction expression.

Effects of evaluated default expressions are effects of the struct construction expression.

Finalization obligations created by supplied field initializer expressions or evaluated default expressions become obligations of the constructed value, local temporaries, or surrounding context according to ownership and lifecycle rules.

A struct construction expression participates in capability checking.

A field initializer can use only the capabilities available in the construction expression’s surrounding context.

A field default can use only the capabilities available to the declaration that defines the default and to the construction context according to the default-expression rules.

Trusted capabilities used by defaults or field initializers must be permitted by the surrounding trusted declaration or rejected according to the contract and trust rules.

A struct construction expression can establish conditions at that program point.

Conditions can include the constructed type, full initialization of the constructed value, initialized fields, and conditions established by field initializer expressions.

Conditions about omitted defaulted fields can be established when the default expression establishes those conditions and the conditions remain valid after construction.

Mutation, movement, consumption, destruction, reinitialization, finalization, or capability loss can invalidate conditions about the constructed value or its fields.

A struct construction expression creates a new value. Initialization performed by the construction expression is initialization, not ordinary mutation.

A value being constructed has no stable observable identity until construction is complete.

Field mutability controls post-initialization mutation of fields. It does not restrict initialization of fields during construction.

```bray
struct Counter
{
    mut value: i64;
}

let c: Counter =
{
    value = 0,
};
```

The field `value` is initialized during construction. Its `mut` field declaration controls later mutation through compatible mutable access paths.

Supplied field initializer expressions are evaluated in source order.

Omitted field defaults are evaluated after supplied field initializers, in field declaration order.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Boolean fold expressions](boolean-fold-expressions.md)
- Next: [Union variant construction expressions](union-variant-construction-expressions.md)
