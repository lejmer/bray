# Lifecycle selection

Lifecycle declaration selection uses the type or implementation subject whose value is being constructed, finalized, destroyed, entered, or exited.

In an inherent implementation, the implementation subject must be the declaring type for type-wide lifecycle declarations.

For a given type, lifecycle kind, and lifecycle path, at most one participating lifecycle declaration can be visible in a coherence domain.

The lifecycle path is:

- the type path for type-wide lifecycle behavior,
- the named constructor path for named constructors,
- the exact trait implementation path for trait implementation `enter` and `exit` fulfillments.

Primary constructors are selected through the type construction surface.

Named constructors are reached through the type path.

```bray
let file = File.temp(directory, prefix = "log");
```

Finalization and destruction are selected from the concrete value being resolved.

Finalization and destruction do not depend on which trait view or trait implementation is used to observe a value.

For named type subjects, finalization and destruction behavior is provided by the type's type-wide lifecycle declarations.

For implementation-eligible type-form subjects, finalization and destruction behavior is the lifecycle behavior produced by that type form.

Scope enter selection uses the initializer value or access path of the `with` expression.

Scope exit selection is the matching exit behavior for the selected enter behavior and scoped capability.

Trait implementation `enter` and `exit` declarations are selected only after the exact trait implementation has been selected.

For a given exact trait implementation and lifecycle requirement, at most one participating implementation lifecycle declaration can be visible in a coherence domain.

## Navigation

- [Language index](../index.md)
- [Lifecycle index](../lifecycle.md)
- Previous: [Lifecycle declarations](lifecycle-declarations.md)
- Next: [Lifecycle ordering](lifecycle-ordering.md)
