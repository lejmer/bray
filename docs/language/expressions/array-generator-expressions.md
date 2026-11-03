# Array generator expressions

Syntax follows the [generator expressions grammar](../syntax-grammar.md#generator-expressions).

An **array generator expression** constructs a fixed-size array by iterating over a source and yielding array elements.

```bray
let xs: [i32; 4] =
[
    each i in 0..4
    {
        yield i * 2;
    }
];
```

An array generator expression is the bracketed array expression form whose top-level child is a generator iteration
expression.

The expression has the form:

```bray
[
    each <pattern> in <source>
    {
        ...
    }
]
```

Array generator source resolution follows [Iteration source resolution](iteration-source-resolution.md).

The pattern is checked against the source element type and must be irrefutable for that type. It can introduce one or
more bindings. Each iteration creates fresh bindings scoped to the iteration body, which are not visible in the source expression.
The bindings are destroyed or ended at the end of each iteration according to ownership, borrowing, destruction, and
finalization rules.

The iteration body is a block expression in yield-capable array-generator context. A `yield` supplies an element to the
nearest enclosing array generator region.

A fixed-size array generator must yield exactly one element per iteration and exactly `N` total elements, where `N` is
the resulting array length. That length may be zero when the source has zero iterations. The compiler must prove the
required cardinality or reject the expression.

The yielded value is checked against the array element type.

Expected array element type can guide literal typing, variant shorthand, struct construction shorthand, box construction
shorthand, conversion checking, and nested expression checking inside yielded expressions.

A yielded value is moved into the array unless it is copied according to the element type’s copy contract or another
explicit rule applies.

The array is fully initialized when every required element has been yielded and initialized.

If iteration exits before the array is fully initialized through `return`, `yield`, `break`, `continue`, `never`,
cancellation, panic, or another control-flow exit, initialized elements and live temporaries are handled by the
corresponding ownership, destruction, and finalization rules.

`continue` targets the nearest iteration region. `break` exits that region and must supply `unit`, because the generator
iteration expression inside an array generator completes as `unit`.

In a fixed-size array generator, any control-flow path that continues an iteration before yielding that iteration’s
required element is rejected unless the compiler can prove the required yield still occurs.

In a fixed-size array generator, any control-flow path that breaks the iteration before yielding every required element
is rejected unless the compiler can prove the required yield count is still satisfied.

Nested yield-capable regions capture their own yields.

```bray
let matrix: [[i32; 2]; 2] =
[
    each row in 0..2
    {
        yield [
            each col in 0..2
            {
                yield row + col;
            }
        ];
    }
];
```

In the nested example, the inner `yield row + col;` supplies the inner array generator, while the outer `yield [...]`
supplies the outer array generator.

Nested array generator expressions are checked recursively.

The compiler checks the effects and capabilities required by the source expression, selected `Iterable` and `Iterator`
operations, pattern operation, iteration body, and yielded expressions against the surrounding context.

Each evaluation of an array generator evaluates its source expression once before iteration, including its effects.
The iteration body's effects occur once per executed iteration.

Finalization obligations created in an iteration body must be completed, transferred, or moved into yielded values
before the iteration body exits.

Finalization obligations of yielded values become part of the resulting array.

Conditions established by the source expression, pattern, and iteration body are scoped according to the iteration
region.

Conditions tied to an iteration binding expire at the end of that iteration unless they are transferred into the yielded
value or another surviving storage location.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Iteration source resolution](iteration-source-resolution.md)
- Next: [General generator expressions](general-generator-expressions.md)
