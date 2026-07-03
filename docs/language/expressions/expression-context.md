# Expression context

Expressions are checked in context.

Expression context can provide:

- expected type,
- expected ownership mode,
- expected borrow mode,
- expected capability mode,
- expected result type,
- expected storage policy type,
- expected union type,
- expected callable type,
- expected effect contract,
- expected pattern operation mode.

Expected type can guide:

- numeric literal typing,
- imaginary literal typing,
- tuple element typing,
- array element typing,
- struct construction shorthand,
- union variant shorthand,
- box construction shorthand,
- conversion checking,
- callable overload resolution.

Example:

```bray
let shape: Shape = .Circle(center = origin, radius = 10.0);
```

The expected type `Shape` lets `.Circle(...)` resolve as a variant construction expression for `Shape`.

Example:

```bray
let node: box List<i32> = box(.Empty);
```

The expected type `box List<i32>` lets `box(...)` expect an inner `List<i32>`, which lets `.Empty` resolve as a variant of `List<i32>`.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Expression results](expression-results.md)
- Next: [Sequenced expressions](sequenced-expressions.md)
