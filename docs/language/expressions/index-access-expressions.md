# Index access expressions

An **index access expression** reaches an indexed element, component, or contiguous sub-storage through an indexing contract.

```bray
items[index]
matrix[row][column]
items[start..end]
items[start..]
items[..end]
items[..]
```

The expression before `[` is the indexed subject.

The content inside `[` is the index selector.

An index selector is either an element selector or a slice selector.

An element selector contains one index expression.

A slice selector uses `..` and denotes a half-open range.

Within brackets, a top-level `..` selects slicing directly. Parentheses make a
[`Range<T>` expression](range-expressions.md) an element selector for a matching custom indexing contract.

The supported slice selector forms are:

```bray
start..end
start..
..end
..
```

The indexed subject is evaluated as an expression and must have a type that supports indexed access.

Indexing and slicing are access-path projection expressions, not unary or binary expressions.

They are not enabled by operator traits.

Selector expressions are evaluated after the indexed subject.

When a selector has both a start expression and an end expression, the start expression is evaluated before the end expression.

Omitted slice boundaries do not evaluate an expression.

Index access with `[]` is asserted access.

When the indexing contract has bounds or validity requirements, the compiler can discharge those requirements from compile-time conditions.

If an asserted index requirement is checked at runtime and fails, the access panics.

Types can provide checked access operations that represent invalid access through ordinary result values.

For fixed-size arrays and slices, an element selector must provide a nonnegative integer index accepted by the indexing contract.

For fixed-size arrays, the valid element index range is `0 <= index < N`, where `N` is the array length.

For slices, the valid element index range is `0 <= index < length`, where `length` is the runtime slice length.

For fixed-size arrays and slices, a slice selector must satisfy `0 <= start <= end <= length`.

The default start boundary is `0`.

The default end boundary is the subject length.

Negative indexing, wraparound indexing, and stride syntax are not part of core `[]` access.

For fixed-size arrays, element access reaches an array element.

```bray
values[index]
```

For nested arrays, repeated element access composes.

```bray
matrix[row][column]
```

The first index access reaches an element of the outer array. The second index access reaches an element of the inner array.

Slicing a fixed-size array or slice reaches contiguous sub-storage with slice type `[T]`.

```bray
let part: &[u8] = &bytes[2..6];
let tail: &[u8] = &bytes[2..];
let prefix: &[u8] = &bytes[..6];
let all: &[u8] = &bytes[..];
```

Slice projection does not copy elements.

Slice projection produces access to contiguous sub-storage. Because `[T]` is unsized, the projected slice must be used through an
indirection boundary such as `&[T]`, `&mut [T]`, or `box[S] [T]`.

Mutable slice borrowing requires mutation authority over the whole projected range.

```bray
let part: &mut [u8] = &mut bytes[2..6];
```

An owned slice storage value can be indexed and sliced through its owned indirection.

```bray
let owned: box[Heap] [u8] = box[Heap]([1, 2, 3, 4]);
let first: &u8 = &owned[0];
let part: &[u8] = &owned[1..4];
```

Index access can produce an access path when the subject expression produces a compatible access path.

An element access path can be observed, borrowed, mutably borrowed, moved from, copied from, consumed, assigned through, or
destroyed according to the subject access path, element type, index contract, ownership state, initialization state, and capability
state.

Observation through index access requires observe capability for the subject and the reached element.

Mutable access through index access requires mutation authority over the subject access path and mutation authority over the
reached element.

Assignment through element access requires the index selector to identify an assignable element access path.

```bray
items[index] = value;
```

Built-in slice projection is not an assignment destination for ordinary `=`.

Modifying projected elements uses element access or operations on a mutable slice borrow.

Moving from an indexed element is an ownership operation.

Moving from an indexed element requires ownership of the reached element and no conflicting active borrows.

Moving an element out of an aggregate can leave the aggregate partially initialized when the aggregate rules permit partial moves.

A partially moved aggregate can be reinitialized or consumed by a rule that accounts for its state.

Destruction of a partially moved aggregate destroys only still-initialized parts.

Copying from an indexed element requires the element type to satisfy the copy contract.

Borrowing an indexed element creates a borrow of the reached element.

Mutable borrowing an indexed element requires compatible exclusivity for the reached element.

Borrowing a slice projection creates a borrow of the projected contiguous substorage.

Mutable borrowing a slice projection requires compatible exclusivity for the whole projected range.

Index access can refine or use conditions at that program point.

Conditions can establish that an index is valid, that a slice range is valid, that an element is initialized, or that an indexed access
is within the subject’s bounds when the indexing contract exposes such conditions.

Mutation, movement, consumption, destruction, reinitialization, or finalization of the subject, reached element, or projected
substorage can invalidate conditions about indexed access.

A custom indexing contract defines:

- the accepted selector shapes,
- the accepted selector expression types,
- the produced value type or access path type,
- the required capabilities,
- the asserted validity requirements,
- the conditions established by successful access,
- the panic condition for failed asserted access.

Custom element indexing uses compiler-known shared and mutable contracts:

```bray
trait ElementIndex<Selector>
{
    type Output;

    func index(pos selector: &Selector) -> &Output;
}

trait MutableElementIndex<Selector>
{
    type Output;

    mut func index(pos selector: &Selector) -> &mut Output;
}
```

Custom slice indexing likewise has shared and mutable contracts:

```bray
trait SliceIndex<Bound>
{
    type Output;

    func slice(pos start: Bound?, pos end: Bound?) -> &Output;
}

trait MutableSliceIndex<Bound>
{
    type Output;

    mut func slice(pos start: Bound?, pos end: Bound?) -> &mut Output;
}
```

The compiler recognizes these exact trait, associated `Output`, and callable declarations by language-defined identity. Declarations
with matching names do not become indexing protocols.

For element indexing, the selector expression supplies `Selector`. For slice indexing, present boundaries supply `Bound` and an
omitted boundary supplies `none`. The `Output` member determines the reached type. Shared observation and shared borrowing select
the shared contract. Assignment and mutable borrowing select the corresponding mutable contract. Each callable returns the borrow
that identifies the reached storage. The callable surface participates in contract and implementation selection, but custom `[]`
remains an access-path projection rather than an ordinary call. The subject's access path and capabilities determine whether the
reached output can be observed, borrowed, moved, or assigned through.

Indexing contract selection is based on the indexed subject type, requested access capability, selector shape, and selector
expression types.

The result type of the index access expression does not select the indexing contract.

Ambiguous indexing contract selection is rejected.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Path expressions](path-expressions.md)
- Next: [Assignment expressions](assignment-expressions.md)
