# Product Types

## Product type

A **product type** is a named type whose value is composed from named fields.

A product type is declared with `struct`.

```bray
struct Point
{
    x: r64;
    y: r64;
}
```

A product value contains one value for each field in the product representation.

The fields together form the product value's primary representation.

Each field has a name, a type, a mutability contract, a visibility contract, and an initialization state.

A product value is fully initialized when all required fields are initialized and all defaulted omitted fields have been
initialized from their defaults during construction.

## Primary representation

A product type has one primary representation declaration.

The primary representation is the field set declared by the `struct` body.

```bray
struct Rectangle
{
    min: Point;
    max: Point;
}
```

The primary representation determines:

- field names,
- field types,
- field declaration order,
- field visibility,
- field mutability,
- field defaults,
- initialization requirements,
- partial-initialization tracking,
- field destruction order,
- field access paths.

Additional inherent implementation blocks owned by the type's defining package can define constructors, methods, static
functions, constants, predicates, type-valued members, and other behavior for the type. Separate trait implementation
blocks can make the type satisfy trait applications.

Inherent and trait implementation blocks do not add fields to the primary representation.

## Field declarations

A field declaration introduces one field of a product type.

```bray
struct Point
{
    x: r64;
    y: r64;
}
```

A field declaration uses:

```bray
name: Type;
```

The field name comes before `:`.

The field type comes after `:`.

A semicolon terminates every field declaration.

The final field declaration also has a semicolon.

```bray
struct Counter
{
    mut value: i64;
}
```

A field declaration can include a default expression.

```bray
struct RetryPolicy
{
    count: i32 = 3;
    delay: Duration = Duration.seconds(1);
}
```

A field declaration can include field mutability.

```bray
struct Counter
{
    mut value: i64;
}
```

A field declaration can include field visibility.

```bray
struct CacheEntry
{
    key: string;
    internal hash: u64;
}
```

The grammar order for field visibility and mutability modifiers is:

```bray
visibility mut name: Type = default;
```

Example:

```bray
struct Example
{
    public mut value: i32;
    internal cache: Cache;
}
```

`public` is optional because it is the default.

## Field names

Field names are part of the product type's representation.

Field names are used by construction expressions, field access expressions, product patterns, documentation, and public
API compatibility.

Field names must be unique within the product type.

A duplicate field name is rejected.

Field names are stable API surface for public product types.

Renaming a public field is a public API change.

## Field order

Field declaration order is semantically relevant for deterministic destruction and any rule that explicitly refers to
declaration order.

Field declaration order can also affect compiler-chosen layout.

Default layout remains compiler-defined.

Construction expressions and product patterns match fields by name.

Field order in construction expressions and product patterns does not determine field identity.

## Field type

A field type determines the type of value stored in that field.

```bray
struct Point
{
    x: r64;
    y: r64;
}
```

The field type participates in:

- field initialization,
- field access,
- field borrowing,
- field mutation,
- field movement,
- field copying,
- field destruction,
- field finalization,
- product copy behavior,
- product lifecycle obligations,
- product layout,
- product construction,
- product patterns.

A field value follows the ownership, borrowing, initialization, destruction, finalization, capability, and effect rules
of its field type.

## Field visibility

Field visibility controls access to the field declaration.

`public` is the default.

```bray
struct Point
{
    x: r64;
    y: r64;
}
```

This is equivalent to:

```bray
struct Point
{
    public x: r64;
    public y: r64;
}
```

An `internal` field is available within its intended scope.

```bray
struct CacheEntry
{
    key: string;
    internal hash: u64;
}
```

Use of an internal field outside its intended scope requires explicit acknowledgement.

```bray
let h = internal entry.hash;
```

A module can acknowledge use of a specific internal module, declaration, or declaration path with `using internal`.

```bray
using internal cache.CacheEntry.hash;
```

`using internal` applies to specific internal modules, declarations, or declaration paths.

`using internal` applies to specific modules, declarations, or declaration paths rather than entire packages.

A public API exposes internal fields only through an explicit public wrapper that removes the internal field from the
public signature.

## Field mutability

Fields are immutable by default after initialization.

```bray
struct Point
{
    x: r64;
    y: r64;
}
```

A field declared with `mut` permits post-initialization mutation through a compatible mutable access path.

```bray
struct Counter
{
    mut value: i64;
}
```

Field mutability and binding mutability are separate.

```bray
let mut point: Point =
{
    x = 1.0,
    y = 2.0,
};
```

The binding `point` has mutable local access authority over the product value.

The fields `x` and `y` remain immutable after initialization because the field declarations do not declare `mut`.

```bray
let mut counter: Counter =
{
    value = 0,
};

counter.value = 1;
```

The assignment is valid when `counter` has mutation authority and `value` is declared `mut`.

Initialization is governed by initialization rules, not by post-initialization field mutability.

A field can be initialized during construction whether or not it is declared `mut`.

## Field defaults

A field default is an expression declared on a field.

```bray
struct RetryPolicy
{
    count: i32 = 3;
    delay: Duration = Duration.seconds(1);
}
```

A field with a default can be omitted during struct construction.

An omitted defaulted field is initialized from its default expression.

Construction-time field default behavior is defined in
[Struct construction expressions](../expressions/struct-construction-expressions.md).

A field default is checked in the struct declaration context.

A field default is a
[declaration-owned runtime default](../declarations/declaration-owned-expressions.md#runtime-defaults). It is checked
even when every current construction supplies that field explicitly.

A field default cannot reference sibling fields.

A field default cannot reference `self`.

A field default is evaluated when that field is omitted during construction.

Effects of an evaluated field default become effects of the construction expression.

Finalization obligations created by an evaluated field default become obligations of the constructed value, local
temporaries, or surrounding context according to ownership and lifecycle rules.

Trusted capabilities used by a field default must be permitted by the declaration context and construction context
according to the trusted capability rules.

A field default participates in type checking, ownership checking, initialization checking, effect checking, capability
checking, finalization tracking, and flow-sensitive contract checking.

## Product construction

Product construction is handled by struct construction expressions.

```bray
let p: Point =
{
    x = 1.0,
    y = 2.0,
};

let p = Point
{
    x = 1.0,
    y = 2.0,
};
```

Product construction expressions initialize product fields and create fully initialized product values.

Struct construction expression rules are defined in
[Struct construction expressions](../expressions/struct-construction-expressions.md).

## Product access paths

A product field access creates an access path to the field.

```bray
point.x
```

The field access path reaches the field storage inside the product value.

Field access uses `.`.

Nested field access composes.

```bray
rectangle.min.x
```

Each path component is checked in order.

The result of each component becomes the subject for the next component.

Observation of a field requires observe capability over the product access path.

Mutation of a field requires:

- mutation authority over the product access path,
- a field declaration that permits mutation,
- no conflicting active borrows,
- any additional constraints imposed by the field type or surrounding context.

Borrowing a field creates a borrow of the reached field storage.

Moving a field out of a product is a partial move of the product.

Copying a field requires the field type to satisfy the copy contract.

## Disjoint field access

Two field access paths into the same product value are disjoint when the compiler can prove they reach distinct fields
and no type-form or representation rule makes them overlap.

Disjoint field access can permit simultaneous compatible operations on distinct fields.

For example, the compiler can reason separately about:

```bray
point.x
point.y
```

when `x` and `y` are distinct fields of the same product value.

Disjointness analysis participates in borrow checking, mutation authority, movement, initialization, destruction, and
condition refinement.

## Product initialization state

A product value has initialization state.

The product as a whole can be uninitialized, partially initialized, fully initialized, moved from, or destroyed.

Each field has its own initialization state while the product is being initialized or after a partial move.

A product value is fully initialized when every field is fully initialized.

A fully initialized product value can be observed, borrowed, moved, copied, consumed, or destroyed as a complete value
according to its type and capability state.

A partially initialized product value can be accessed only through initialized parts when the operation permits
partial-state access.

A moved-from product value can be reinitialized when the storage and type contract permit it.

A destroyed product value is no longer usable as a value.

## Partial moves

Moving a field out of a product is an ownership operation.

A field move requires ownership of the containing product value.

A field move requires no conflicting active borrows of the containing product value or the reached field.

After a field move, the containing product value is partially initialized.

The moved field becomes moved from within the product.

Still-initialized fields remain governed by their own ownership and destruction rules.

A partially moved product value can be reinitialized or consumed by a rule that accounts for its partial state.

A partially moved product value can be destroyed as a partial value.

Destruction of a partially moved product value destroys only the still-initialized fields.

A partially moved product value can become fully initialized again when all moved-from fields are reinitialized and the
storage and type contract permit reinitialization.

A product type with whole-product lifecycle behavior must be fully initialized whenever a whole-product lifecycle
declaration can run.

A field move from such a product is valid only when every reachable path re-initializes the field before:

- the product is finalized,
- the product is destroyed as a complete value,
- the product is used as a `with` initializer,
- the product is moved, copied, consumed, borrowed, or observed as a complete value,
- ownership of the product can end.

If the compiler cannot prove that the product becomes fully initialized before one of those events, the field move is
rejected.

Partial product storage resolves only initialized fields.

Partial product storage does not run whole-product finalizers, whole-product destructors, or whole-product scope
enter/exit behavior.

## Product movement

Moving a fully initialized product value moves the complete product.

Moving the complete product transfers ownership of every initialized field to the new owner.

The old access path becomes moved-from until reinitialized.

Moving a product preserves the product's type and field structure.

A product move transfers finalization obligations carried by the product or its fields to the new owner.

## Product copying

A product value is copyable only when the product type has an accepted `@copy` contract and every field satisfies the
required copy contract for that concrete type.

Copying a product copies every field according to its field type's copy contract.

Copying a product produces a separate value with its own ownership story.

Copying a product preserves the abstract value according to the product's copy contract.

Copy behavior is explicit through the product type's `@copy` contract.

## Product destruction

Destroying a fully initialized product value runs any whole-product destructor before field destruction.

Initialized fields are then destroyed.

Fields are destroyed in Bray field destruction order.

The default product field destruction order is reverse declaration order.

```bray
struct Example
{
    first: Resource;
    second: Resource;
}
```

For this type, `second` is destroyed before `first` under default destruction order.

A partially initialized product destroys only initialized fields.

A moved-from field is not destroyed by the old product owner.

A product type can define a destructor with a `destruct` lifecycle declaration.

Product destructor declarations follow the general [destruction](../lifecycle/destruction.md) rules.

Product finalization obligations follow the general [finalization](../lifecycle/finalization.md) rules.

## Product lifecycle declarations

A product type can define lifecycle declarations inside its type body or inside an inherent implementation for the
product type.

```bray
struct File
{
    handle: OsHandle;

    construct(pos path: Path, mode: FileMode = FileMode.read) -> Self
    {
        ...
    }

    construct temp(pos directory: Path, prefix: string = "tmp") -> Self
    {
        ...
    }

    async finalize() -> Result<unit, FileError>
    {
        ...
    }

    destruct()
    {
        ...
    }
}
```

Product constructors create fully initialized values of the product type.

Named product constructors are reached through the product type path:

```bray
let my_file = File.temp(some_path);
```

Product lifecycle declarations follow the [Lifecycle](../lifecycle.md) ordering, signature, and selection rules.

Product lifecycle declarations are whole-product lifecycle declarations.

A successfully constructed product carries:

- the lifecycle obligations declared by the product type,
- the lifecycle obligations of its initialized fields,
- any lifecycle obligations produced by field defaults or constructor body expressions.

Field defaults used during construction are evaluated according to product construction rules before the product becomes
fully initialized.

If a destructor consumes or destroys a field, that field becomes uninitialized and is not destroyed again after the
destructor returns.

Any initialized fields remaining after the destructor returns are destroyed in product field destruction order.

The scoped capability can borrow from the product, carry access authority for the product, or carry an independent
resource token, according to the scoped capability type.

An active scoped capability can restrict observation, mutation, borrowing, movement, finalization, destruction, and
partial moves of the product for the lifetime of the `with` body.

A whole-product assignment or replacement resolves the old product value according to product finalization, destruction,
and field destruction rules before the new product value becomes initialized at that access path.

## Product layout

The default physical layout of a product type is compiler-defined.

Product layout is declared with `@layout(...)` immediately before the `struct` declaration.

```bray
@layout(stable)
struct Header
{
    magic: u32;
    version: u16;
}
```

Product layout modes, options, default layout behavior, transparent layout, and product-specific layout rules are
defined in [Product layout](../targets-layout-abi-and-raw-memory/product-layout.md).

## Product patterns

Product values can be decomposed by product patterns.

```bray
let { x, y }: Point = point;
```

Product patterns match fields by name.

Field order does not matter.

Field shorthand binds a field to a binding with the same name. The shorthand binding name is not resolved as a named
constant or variant.

```bray
{ x, y }
```

`..` explicitly accounts for remaining fields and introduces no bindings.

```bray
{ x, .. }
```

Product pattern rules are defined in [Product patterns](../patterns/product-patterns.md).

Product patterns participate in ownership, borrowing, copying, partial moves, initialization, destruction, finalization,
capability checking, and condition refinement according to the pattern operation mode.

## Product API compatibility

For a public product type, public fields are part of the public API.

Adding, removing, renaming, or changing the type of a public field is a public API change.

Changing field mutability is a public API change.

Changing field visibility is a public API change.

Changing field defaults can be a public API change when construction behavior visible to users changes.

Changing product lifecycle declarations follows the public API compatibility rule defined in
[API compatibility](../lifecycle/api-compatibility.md).

Default physical layout is not public ABI unless the type declares an explicit layout contract.

## Navigation

- [Language index](../index.md)
- [Types index](../types.md)
- Previous: [Lifecycle declarations](lifecycle-declarations.md)
- Next: [Union Types](union-types.md)
