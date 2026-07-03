# Box construction expressions

A **box construction expression** creates an owned indirection value.

A default-storage box construction expression uses `box(...)`.

```bray
let node: box List<i32> = box(List<i32>.Empty);
```

An explicit-storage box construction expression uses `box[S](...)`, where `S` is the storage policy type.

```bray
let node: box[Heap] List<i32> = box[Heap](List<i32>.Empty);
```

A box construction expression produces a value whose type uses the `box` type form.

```bray
box[S] T
```

`S` is the storage policy type.

`T` is the contained subject type.

`box T` uses the default storage policy.

`box[S] T` uses storage policy type `S`.

For sized `T`, the storage policy type must satisfy `Storage<T>`.

For `box[S] view TraitApplication`, the storage policy type must satisfy `Storage<U>` for the sized concrete source type `U` used to form the view.

For `box[S] [T]`, the storage policy type must provide contiguous owned storage behavior for element type `T` and a runtime element
count.

A `box[S](value, ...)` expression constructs a `box[S] T` from a contained value of type `T`.

When the expected box type is `box[S] view TraitApplication`, the contained value expression can have a sized concrete type `U` that satisfies the exact trait application.

In that case, box construction stores `U` through `Storage<U>` and forms the resulting box view with the selected `U(TraitApplication)` implementation witness.

When the expected box type is `box[S] [T]`, the contained value expression must produce an owned contiguous sequence of `T`
elements with a known finite element count at construction time.

```bray
let bytes: box[Heap] [u8] = box[Heap]([1, 2, 3, 4]);
```

In that case, box construction allocates contiguous storage for the element count, initializes each element in order, records the
runtime length, and forms the resulting owned slice storage.

The contained value expression is the first runtime argument to the box construction expression.

Additional runtime arguments follow the selected storage construction parameter declarations.

Storage-policy arguments can be positional only when the corresponding storage construction parameter is marked `pos`.

```bray
let point: box[AllocatorStorage<MyAllocator>] Point =
    box[AllocatorStorage<MyAllocator>](
        { x = 1.0, y = 2.0, },
        storage = storage,
    );
```

A box construction expression without explicit `[S]` uses the expected box type when one is available.

```bray
let node: box[Heap] List<i32> = box(List<i32>.Empty);
```

Here the expected type provides storage policy `Heap` and contained type `List<i32>`.

A box construction expression can propagate expected contained type into the contained value expression.

```bray
let node: box List<i32> = box(.Empty);
```

Here the expected type `box List<i32>` gives `box(...)` the contained expected type `List<i32>`, which lets `.Empty` resolve as a variant of `List<i32>`.

When no expected box type is available and no explicit storage policy is supplied, the box construction expression uses the default storage policy and infers the contained type from the contained value expression.

When no expected box type is available and an explicit storage policy is supplied, the contained type is inferred from the contained value expression.

The contained value expression is checked against the expected contained type when one is available.

When the expected contained type is a trait view, the contained value expression is checked as a view-formation source rather than as a value whose type is exactly the view type.

The expected contained type can guide literal typing, union variant shorthand, struct construction shorthand, tuple element typing, array element typing, conversion checking, and nested expression checking.

The storage policy runtime arguments are checked against the storage construction behavior required by the selected storage policy type.

A storage policy argument must correspond to a declared parameter required by the storage construction behavior.

Duplicate storage policy arguments are errors.

Unknown storage policy arguments are errors.

Missing required storage policy arguments are errors unless the corresponding parameter has a default.

Box construction evaluates the contained value expression and initializes indirect storage with that value.

The contained value is moved into the box storage unless the value is copied according to its type’s copy contract or another explicit rule applies.

For sized `T`, the resulting `box[S] T` owns the indirect storage and the contained `T`.

For `box[S] view TraitApplication`, the resulting box owns the stored concrete `U` and exposes it through `view TraitApplication`.

For `box[S] [T]`, the resulting box owns the contiguous element storage and exposes it through `[T]`.

Moving a `box[S] T` moves ownership of the indirection value.

Destroying a `box[S] T` destroys the stored value and releases storage according to the storage policy.

Borrowing a `box[S] T` can project borrows of the contained or viewed value according to the box type form and storage behavior.

A mutable borrow of a `box[S] T` can project mutable access to the contained or viewed value when the box access path, storage policy, and contained type permit it.

A box construction expression is fully initialized when the storage state has been created, the contained value has been initialized in the storage, and the `box[S] T` value has been formed.

If evaluation exits before box construction completes, already-initialized values, storage state, and temporaries are handled by the corresponding control-flow, ownership, destruction, finalization, and storage-release rules.

A box construction expression participates in effect checking and capability checking.

Effects of the contained value expression are effects of the box construction expression.

Effects of storage construction behavior are effects of the box construction expression.

Finalization obligations created by the contained value expression, storage construction behavior, or contained value become obligations of the resulting box, local temporaries, or surrounding context according to ownership and lifecycle rules.

Trusted capabilities used by storage construction behavior must be permitted by the trusted declaration that implements that behavior or rejected according to the Contract and Trust Model.

A box construction expression can establish facts in the fact context.

Facts can include the produced box type, storage policy type, contained or viewed type, stored concrete type when it remains visible to the checking context, full initialization of the box value, initialization of the stored value, and facts established by the contained value expression or storage construction behavior.

Mutation, movement, consumption, destruction, reinitialization, finalization, or capability loss can invalidate facts about the box value, storage state, or contained value.

The contained value expression and additional explicit storage policy arguments are evaluated in source order.

Omitted storage construction defaults are evaluated after explicit runtime arguments, in storage construction parameter declaration order.

The box construction operation itself initializes indirect storage with the contained value before the resulting box value becomes available.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Union variant construction expressions](union-variant-construction-expressions.md)
- Next: [Type-form construction expressions](type-form-construction-expressions.md)
