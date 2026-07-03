# Pattern operation modes

A pattern can be applied in different operation modes.

The operation mode is supplied by the construct using the pattern.

The core modes are:

```text
observe
shared borrow
mutable borrow
consume
copy
```

In **observe mode**, the pattern refines the subject and binds observed access paths.

In **shared borrow mode**, the pattern binds shared borrowed access paths.

In **mutable borrow mode**, the pattern binds mutable borrowed access paths when the subject and field contracts permit mutation authority.

In **consume mode**, the pattern moves owned parts out according to ownership rules.

In **copy mode**, the pattern copies matched parts when the type's copy contract permits it.

The same pattern syntax can be used in multiple operation modes. The surrounding construct decides how the pattern accesses or extracts the matched parts.

## Navigation

- [Language index](../index.md)
- [Patterns index](../patterns.md)
- Previous: [Pattern contexts](pattern-contexts.md)
- Next: [Partial moves through patterns](partial-moves-through-patterns.md)
