# Binding patterns

## Discard pattern

The discard pattern matches the subject and introduces no binding.

```bray
_
```

The discard pattern is irrefutable.

It is used to match a value or part of a value without naming it.

## Mutable binding pattern

A mutable owned binding pattern uses `mut` before the binding name.

```bray
mut value
```

`mut value` introduces a mutable owned local binding when the pattern operation produces an owned value.

The `mut` applies to the binding introduced by the pattern. It does not change the mutability of the original subject.

The pattern operation determines whether the binding receives an owned value, a copy, or an access path.

## Pattern bindings

A binding introduced by a pattern can bind a value or an access path.

The pattern operation mode determines the binding kind.

For example, a binding pattern can introduce:

```text
an owned value,
a copied value,
a shared borrowed access path,
a mutable borrowed access path,
or an observed access path.
```

Every pattern binding has a type, lifetime, capability set, initialization state, and ownership story.

Pattern bindings are scoped to the region introduced by the construct that applied the pattern.

## Navigation

- [Language index](../index.md)
- [Patterns index](../patterns.md)
- Previous: [Pattern resolution](pattern-resolution.md)
- Next: [Literal patterns](literal-patterns.md)
