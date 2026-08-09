# Local binding declarations inside block expressions

A **local binding declaration** introduces one or more local bindings inside a block expression.

A local binding declaration uses `let`.

```bray
let x: i32 = 1;
let mut y: i32 = 2;
```

A local binding declaration contains a pattern.

```bray
let pattern = initializer;
let pattern: Type = initializer;
```

The pattern determines which bindings are introduced.

A simple binding is a binding pattern.

```bray
let x: i32 = 1;
```

A mutable local binding is a mutable binding pattern.

```bray
let mut x: i32 = 1;
```

`mut` before a binding name applies to the local owned binding introduced by the pattern.

A local binding declaration can destructure with any irrefutable pattern accepted for the initializer type.

```bray
let (x, y): (i32, i32) = pair;

let { x, y }: Point = point;

let [first, second, third]: [i32; 3] = values;
```

A local binding declaration requires an irrefutable pattern.

A refutable pattern belongs to match expressions or another construct that defines behavior for failed matching.

A union variant pattern is valid in a local binding declaration only when it is irrefutable for the subject type.

A product pattern is irrefutable when all selected subpatterns are irrefutable and the product shape is guaranteed by the subject type.

A tuple pattern is irrefutable when all element subpatterns are irrefutable and the tuple arity matches the subject type.

A fixed-size array pattern is irrefutable when it accounts for the fixed array shape and every listed element pattern is irrefutable.

The initializer expression is evaluated once.

The initializer expression is checked against the annotated type when a type annotation is present.

The type annotation on a local binding declaration applies to the initializer subject matched by the pattern.

```bray
let { x, y }: Point = point;
```

Here `Point` is the expected type of `point` and the subject type for the pattern `{ x, y }`.

When a type annotation is absent, the initializer expression determines the subject type, and the pattern is checked against that subject type.

```bray
let { x, y } = point;
```

The pattern can provide expected type context to subexpressions only through the subject type established by annotation, initializer type, or surrounding context.

A local binding declaration completes when the initializer has been evaluated, the pattern has matched, and every binding introduced by the pattern has been initialized.

Bindings introduced by the pattern become visible after the local binding declaration completes.

Bindings introduced by the pattern are not visible inside the initializer expression.

A pattern-introduced binding name must be unique within that pattern.

A local binding declaration introduces one or more unqualified lookup names into the current scope.

Each introduced unqualified lookup name must not already resolve in the ordinary lookup namespace from that scope.

A local binding declaration introduces each pattern binding exactly once.

A binding cannot be rebound.

A binding is a name for an access path, value, or other declaration result. The binding is not itself storage.

A local value binding introduces an access path to storage with the declared or inferred capability.

A binding pattern without `mut` introduces an immutable binding.

```bray
let x = value;
```

A mutable binding pattern introduces a mutable owned local binding.

```bray
let mut x = value;
```

A mutable owned local binding grants mutation authority over the local access path, subject to the type’s field, payload, and representation contracts.

Field mutability still applies.

```bray
struct Point
{
    x: r64;
    y: r64;
}

let mut p: Point = { x = 1.0, y = 2.0, };
```

The binding `p` is mutable, but fields declared without `mut` remain immutable after initialization.

A borrow type controls access to reached storage through the borrow layer.

```bray
let r: &mut Buffer = &mut buffer;
```

The `&mut` type form and borrow expression grant mutation authority over the reached storage according to Bray's borrowing rules.

`mut` before a binding name controls local owned binding authority.

The initializer expression can produce an owned value, copied value, borrowed value, access path, or other expression result accepted by the pattern context.

For local binding declarations, the pattern operation mode initializes local bindings from the initializer result.

If the initializer result is an owned value, the pattern can move parts of that value into the introduced bindings.

If the initializer result is copyable and the context selects copy behavior, the pattern can copy parts into introduced bindings.

If the initializer result is a borrow value, the pattern can introduce bindings to borrowed access paths according to the borrow type and pattern operation mode.

If the initializer result is an access path to an existing owned value, using it as an owned initializer moves from that access path unless the type is copyable or the expression explicitly borrows.

```bray
let b = a;
```

This moves `a` into `b` when `a` is non-copyable.

The old access path `a` becomes moved-from until reinitialized.

A destructuring local binding can partially move from an existing access path.

```bray
let { x, y } = point;
```

If `point` is an owned non-copyable value and the pattern moves fields out, `point` becomes partially initialized after the declaration unless the entire value is consumed by the destructuring rule.

A partially moved value cannot be used as a complete value until reinitialized or consumed by a rule that accounts for its state.

Destruction of a partially moved value destroys only the still-initialized parts.

When the initializer is a temporary value produced solely for the local binding declaration, destructuring consumes that temporary into the introduced bindings, and no remaining named subject exists after the declaration.

Product patterns in local binding declarations match fields by name.

```bray
let { x, y }: Point = point;
```

Field order does not matter.

Duplicate fields are errors.

Unknown fields are errors.

Missing fields are errors unless `..` is present.

Field shorthand introduces same-name bindings. A field shorthand binding name is not resolved as a named constant or variant.

```bray
let { x, y }: Point = point;
```

This means the fields `x` and `y` are bound as local bindings named `x` and `y`.

`..` explicitly accounts for remaining fields and introduces no bindings.

```bray
let { x, .. }: Point = point;
```

Tuple patterns in local binding declarations match tuple elements by position.

```bray
let (left, right): (i32, i32) = pair;
```

Tuple arity must match the subject tuple type.

Each element pattern is checked against the corresponding tuple element type.

A one-element tuple pattern uses a trailing comma.

```bray
let (single,): (i32,) = tuple;
```

Fixed-size array patterns in local binding declarations match array elements by position.

```bray
let [first, second, third]: [i32; 3] = values;
```

The number of listed element patterns must match the array length unless `..` is present.

```bray
let [first, .., last]: [i32; 4] = values;
```

Each listed element pattern is checked against the array element type.

`..` explicitly accounts for remaining elements and introduces no bindings.

Box patterns in local binding declarations match through owned indirection when the subject type is `box[S] T`.

```bray
let box(inner): box[Heap] Node = node;
```

The inner pattern is checked against `T`.

The local binding declaration’s operation mode determines whether the contained value is observed, borrowed, copied, or consumed.

A consuming `box(inner)` pattern consumes the box and moves through the owned indirection according to `box` ownership rules.

A borrowing `box(inner)` pattern projects a borrow of the contained value according to the box storage policy and the required storage behavior.

A local binding declaration can establish conditions at that program point.

Conditions established by local binding patterns can include product field availability, tuple shape, fixed array shape, active union variant when the pattern is irrefutable for the subject, payload initialization, literal equality when an irrefutable literal context exists, and initialized local bindings.

Guarantees established by the initializer expression remain available after the declaration when they remain valid.

Mutation, movement, consumption, destruction, reinitialization, finalization, or capability loss can invalidate conditions established by a local binding declaration.

A local binding declaration participates in finalization tracking.

If an introduced binding owns a value with a finalization obligation, the obligation is tracked from the point the binding is initialized.

The binding must be finalized, transferred to another owner that assumes the obligation, or converted into an explicit fallback ownership form before the owning scope exits.

A local binding declaration participates in destruction.

Each owned binding introduced by the pattern is destroyed when its owning scope exits, unless ownership has moved elsewhere or the value has entered another ownership construct.

A binding moved out before scope exit is not destroyed by the old binding.

A partially initialized binding destroys only initialized parts.

If initializer evaluation leaves the declaration before it completes, the pattern bindings are not introduced.

This includes `return`, `yield` to an enclosing yield-capable region, `break`, `continue`, nullable propagation, result
propagation, run-result propagation, uncaught panic propagation, cancellation, and any expression path with type `never`.

A caught panic inside the initializer does not leave the declaration; it produces the catch expression's normal result.

Values and temporaries already initialized during initializer evaluation are handled by the corresponding control-flow,
ownership, destruction, and finalization rules.

A local binding declaration is a declaration inside a block expression, but its initializer and pattern matching are expression-checked operations.

Local binding declarations participate in block-expression sequencing and must be terminated according to the block expression’s sequencing rules.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Callable-body block expressions](callable-body-block-expressions.md)
- Next: [Literal expressions](literal-expressions.md)
