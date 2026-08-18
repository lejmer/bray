# Lifecycle declarations

Lifecycle declarations attach construction, finalization, destruction, or scoped-use behavior to a type or
implementation subject.

Lifecycle declarations can appear in:

- type bodies that accept lifecycle declarations,
- inherent implementation bodies for the declaring type,
- trait bodies as lifecycle requirements,
- trait implementation bodies for `enter` and `exit` requirements.

Type-body and inherent implementation lifecycle declarations are type-wide lifecycle declarations.

Trait lifecycle requirements describe lifecycle behavior that an implementing subject must satisfy.

Trait implementation lifecycle declarations can define only `enter` and `exit` fulfillments required by the trait being
implemented.

`Self` in a lifecycle declaration means the declaring type or implementation subject according to the declaration
context.

Lifecycle declarations use callable parameter lists, callable result clauses, callable contract clauses, and callable
bodies where the declaration form permits a body.

Constructors use `construct`.

```bray
construct(pos path: Path, mode: FileMode = FileMode.read) -> Self
{
    ...
}
```

A constructor with no name after `construct` is the primary constructor form.

A constructor with a name after `construct` becomes a named constructor under the type.

```bray
construct temp(pos directory: Path, prefix: string = "tmp") -> Self
{
    ...
}
```

Finalizers use `finalize`.

```bray
async finalize() -> Result<unit, FileError>
{
    ...
}
```

Destructors use `destruct`.

```bray
destruct()
{
    ...
}
```

Scoped enter and exit declarations use `enter` and `exit`.

```bray
enter() -> FileLease
{
    ...
}

exit(pos lease: FileLease)
{
    ...
}
```

Finalizers, scope enter declarations, and scope exit declarations can be synchronous or asynchronous.

Constructors and destructors are synchronous.

Destructors are infallible and return `unit`.

Finalizers, destructors, and scope exit declarations can omit the result clause when the result type is `unit`.

Scope enter declarations require a result clause because the result type is the scoped capability value made available
to the `with` body.

Scope exit declarations take exactly one scoped-capability parameter.

Constructors have no receiver and no `self` binding.

Finalizers have an implicit mutable receiver. Destructors have an implicit consuming mutable receiver.

Scope enter declarations use the same receiver modifiers as instance methods. The default receiver is shared. `mut`,
`consume`, and `consume mut` select the corresponding receiver modes.

Scope exit declarations have no receiver. Their scoped-capability parameter is their only value input.

Each lifecycle member definition has a callable body block.

Lifecycle requirements in traits end with `;`.

## Navigation

- [Language index](../index.md)
- [Lifecycle index](../lifecycle.md)
- Previous: [Overview](overview.md)
- Next: [Lifecycle selection](lifecycle-selection.md)
