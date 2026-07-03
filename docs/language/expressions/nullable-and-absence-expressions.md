# Nullable and absence expressions

The nullable type form is `T?`.

The absence expression is `none`.

`none` has no standalone type.

It is accepted only when the surrounding context determines a concrete nullable type `T?`.

```bray
let mut count: i32? = none;
count = 10;
count = none;
```

Nullable initialization and assignment rules are defined by the [nullable type form](../types/type-forms.md#nullable-type-form).

Nullable patterns match nullable state in pattern-bearing expressions.

```bray
match count
{
    case ?value
    {
        yield value;
    }
    case none
    {
        yield 0;
    }
}
```

`none` matches absent state.

`?pattern` matches present state and applies `pattern` to the contained `T`.

`?pattern` is valid only when the subject type is `T?`.

Bare binding patterns bind the whole nullable value.

Nullable patterns are the ordinary unwrapping mechanism.

There is no separate forced unwrap expression.

A value of type `T?` can be unwrapped to `T` only by proving present state through a nullable pattern or by using nullable
propagation.

The nullable propagation expression is:

```bray
expression?
```

The operand expression must have type `T?`.

The normal continuation of `expression?` has type `T`.

If the operand is present, `expression?` evaluates to the contained `T`.

If the operand is absent, `expression?` propagates `none` to the nearest enclosing nullable propagation boundary.

A nullable propagation boundary is:

- a callable execution scope whose result type is `R?`,
- a single-yield region whose result type is `R?`.

If no nullable propagation boundary is available, `expression?` is rejected.

On the absent path, propagation supplies `none` to the target boundary and the current control-flow path has no normal continuation.

For a callable execution scope, this has the same boundary behavior as returning `none`.

For a single-yield region, this has the same boundary behavior as yielding `none` to that region.

Nested callable execution scopes and nested yield-capable regions create their own propagation boundaries when their result type is
nullable.

The operand is evaluated exactly once.

There is no implicit propagation through field access, method calls, function calls, indexing, or construction.

```bray
func full_name(user_id: UserId) -> string?
{
    let user = find_user(user_id)?;
    let profile = user.profile?;
    return profile.full_name;
}
```

In type context, postfix `?` is the [nullable type form](../types/type-forms.md#nullable-type-form).

In expression context, postfix `?` is nullable propagation.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Assertion expressions](assertion-expressions.md)
- Next: [Result and run result propagation expressions](result-and-run-result-propagation-expressions.md)
