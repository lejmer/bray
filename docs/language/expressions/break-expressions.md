# Break expressions

A **break expression** exits the nearest loop or iteration region that accepts `break`.

The break expression forms are:

```bray
break value;
```

```bray
break;
```

`break` targets the nearest compatible loop or iteration region.

The operand of `break value` is evaluated exactly once.

The break value must be compatible with the target region's result type.

`break;` is shorthand for `break unit;`.

At the target region, `break` is a normal exit path that supplies the break value as the target region's result.

`break` has type `never` because the current normal continuation does not run.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Catch expressions](catch-expressions.md)
- Next: [Continue expressions](continue-expressions.md)
