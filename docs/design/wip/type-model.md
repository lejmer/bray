# Type Model

## Overview

A **type** is a semantic contract for values, storage, access paths, operations, initialization, ownership, borrowing, mutation authority, destruction, finalization, effects, capabilities, and representation.

Every value has a type.

Every expression has a type or produces a compile-time entity governed by type-checking rules.

Every access path has a type and a capability state.

A type determines which operations are valid for values of that type and which obligations those operations create.

Types participate in:

- initialization checking,
- ownership checking,
- movement,
- copying,
- borrowing,
- mutation authority,
- destruction,
- finalization,
- pattern checking,
- construction,
- conversion,
- callable checking,
- contract checking,
- fact-context refinement,
- layout selection,
- public API compatibility.

A type can define structure, behavior, lifecycle rules, construction rules, conversion rules, visibility rules, and layout contracts.

---

## Type identity

A named type has identity.

Two named types with the same representation are still distinct types unless a declared conversion, behavioral contract, or other language rule relates them.

A structural type form produces a type according to its type-form rules.

Examples:

```bray
(i32, i32)
[i32; 4]
box[Heap] Point
func(left: i32, right: i32) -> i32
```

The identity of a type-form type includes the type form and its type-form arguments.

For example:

```bray
box[Heap] Point
box[ArenaStorage] Point
```

are distinct types because the storage policy argument differs.

---

## Type categories

Bray has multiple type categories.

The currently defined categories include:

- scalar types,
- product types,
- union types,
- tuple types,
- fixed-size array types,
- slice types,
- nullable types,
- borrow types,
- trait-view types,
- owned-indirection types,
- callable types.

Scalar types include integers, real floating-point types, complex floating-point types, machine-sized integer types, `bool`, `char`, `unit`, and `never`.

Product types are named types with fields.

Union types are closed tagged sum types with variants.

Compiler-known result types are named union types with language-defined variant contracts.

Compiler-known task handle types are linear ownership types with language-defined async contracts.

Tuple types are fixed-size ordered product types.

Fixed-size array types are fixed-size ordered homogeneous product types.

Slice types are unsized contiguous sequence types.

Nullable types are produced by the postfix nullable type form `T?`.

Borrow types are produced by `&T` and `&mut T`.

Trait-view types are produced by the `view` type form.

Owned-indirection types are produced by the `box` type form.

Callable types are produced by the `func(...) -> ...` type form.

TODO: Define category-specific rules for scalar types and any categories not specified in this model.

---

## Type declarations

A **type declaration** introduces a named type.

Product types are declared with `struct`.

Union types are declared with `union`.

```bray
struct Point
{
    x: r64;
    y: r64;
}

union Shape
{
    Circle(center: Point, radius: r64);
    Rectangle(min: Point, max: Point);
    Empty;
}
```

A type declaration defines the type’s primary semantic surface.

A type declaration can contain representation members, lifecycle declarations, constructors, and other type-owned declarations according to the rules for that type category.

A type declaration can be `public` or `internal`.

`public` is the default visibility.

```bray
public struct Point
{
    x: r64;
    y: r64;
}

internal struct ParserState
{
    position: usize;
}
```

A public type is part of the public API surface of its module or package.

An internal type is available within its intended scope.

Use outside that scope requires explicit internal-use acknowledgement.

---

## Generic types

A type declaration can be generic.

```bray
struct Pair<TLeft, TRight>
{
    left: TLeft;
    right: TRight;
}
```

A generic type declaration is parameterized by declared generic parameters.

TODO: Define generic parameter kinds beyond type parameters.

The body of a generic type declaration is checked against its declared parameters and constraints.

A concrete instantiation supplies arguments for the generic parameters.

```bray
Pair<i32, r64>
```

TODO: Define any remaining generic checking rules not covered by declared parameters and `with(...)` constraints.

---

## Product Types

### Product type

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

The fields together form the product value’s primary representation.

Each field has a name, a type, a mutability contract, a visibility contract, and an initialization state.

A product value is fully initialized when all required fields are initialized and all defaulted omitted fields have been initialized from their defaults during construction.

### Primary representation

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

Additional implementation blocks can define constructors, methods, static functions, trait implementations, and other behavior for the type.

Additional implementation blocks do not add fields to the primary representation.

### Field declarations

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
    key: String;
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

### Field names

Field names are part of the product type’s representation.

Field names are used by construction expressions, field access expressions, product patterns, diagnostics, documentation, and public API compatibility.

Field names must be unique within the product type.

A duplicate field name is rejected.

Field names are stable API surface for public product types.

Renaming a public field is a public API change.

### Field order

Field declaration order is semantically relevant for deterministic destruction and any rule that explicitly refers to declaration order.

Field declaration order can also affect compiler-chosen layout.

Default layout remains compiler-defined.

Construction expressions and product patterns match fields by name.

Field order in construction expressions and product patterns does not determine field identity.

### Field type

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

A field value follows the ownership, borrowing, initialization, destruction, finalization, capability, and effect rules of its field type.

### Field visibility

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
    key: String;
    internal hash: u64;
}
```

Use of an internal field outside its intended scope requires explicit acknowledgement.

```bray
let h = internal entry.hash;
```

A module can acknowledge use of a specific internal declaration or declaration path with `using internal`.

```bray
using internal cache.CacheEntry.hash;
```

`using internal` applies to specific internal declarations or declaration paths.

`using internal` applies to declarations or declaration paths rather than entire modules or packages.

A public API exposes internal fields only through an explicit public wrapper that removes the internal field from the public signature.

### Field mutability

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

### Field defaults

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

A supplied field initializer suppresses evaluation of that field’s default expression.

A field default is checked in the struct declaration context.

A field default cannot reference sibling fields.

A field default cannot reference `self`.

A field default is evaluated when that field is omitted during construction.

Effects of an evaluated field default become effects of the construction expression.

Finalization obligations created by an evaluated field default become obligations of the constructed value, local temporaries, or surrounding context according to ownership and lifecycle rules.

Trusted capabilities used by a field default must be permitted by the declaration context and construction context according to the trusted capability rules.

A field default participates in type checking, ownership checking, initialization checking, effect checking, capability checking, finalization tracking, and fact-context behavior.

### Product construction

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

Construction expressions initialize fields by name.

Construction field order does not matter.

All non-default fields must be initialized.

Omitted defaulted fields use their declared defaults.

A product construction expression creates a fully initialized product value when all fields have been initialized.

Detailed construction-expression rules belong to the Expression Model.

### Product access paths

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

### Disjoint field access

Two field access paths into the same product value are disjoint when the compiler can prove they reach distinct fields and no type-form or representation rule makes them overlap.

Disjoint field access can permit simultaneous compatible operations on distinct fields.

For example, the compiler can reason separately about:

```bray
point.x
point.y
```

when `x` and `y` are distinct fields of the same product value.

Disjointness analysis participates in borrow checking, mutation authority, movement, initialization, destruction, and fact-context refinement.

### Product initialization state

A product value has initialization state.

The product as a whole can be uninitialized, partially initialized, fully initialized, moved from, or destroyed.

Each field has its own initialization state while the product is being initialized or after a partial move.

A product value is fully initialized when every field is fully initialized.

A fully initialized product value can be observed, borrowed, moved, copied, consumed, or destroyed as a complete value according to its type and capability state.

A partially initialized product value can be accessed only through initialized parts when the operation permits partial-state access.

A moved-from product value can be reinitialized when the storage and type contract permit it.

A destroyed product value is no longer usable as a value.

### Partial moves

Moving a field out of a product is an ownership operation.

A field move requires ownership of the containing product value.

A field move requires no conflicting active borrows of the containing product value or the reached field.

After a field move, the containing product value is partially initialized.

The moved field becomes moved from within the product.

Still-initialized fields remain governed by their own ownership and destruction rules.

A partially moved product value can be reinitialized or consumed by a rule that accounts for its partial state.

A partially moved product value can be destroyed as a partial value.

Destruction of a partially moved product value destroys only the still-initialized fields.

A partially moved product value can become fully initialized again when all moved-from fields are reinitialized and the storage and type contract permit reinitialization.

A product type with whole-product lifecycle behavior must be fully initialized whenever a whole-product lifecycle declaration can
run.

A field move from such a product is valid only when every reachable path re-initializes the field before:

- the product is finalized,
- the product is destroyed as a complete value,
- the product is used as a `with` initializer,
- the product is moved, copied, consumed, borrowed, or observed as a complete value,
- ownership of the product can end.

If the compiler cannot prove that the product becomes fully initialized before one of those events, the field move is rejected.

Partial product storage resolves only initialized fields.

Partial product storage does not run whole-product finalizers, whole-product destructors, or whole-product scope enter/exit
behavior.

### Product movement

Moving a fully initialized product value moves the complete product.

Moving the complete product transfers ownership of every initialized field to the new owner.

The old access path becomes moved-from until reinitialized.

Moving a product preserves the product’s type and field structure.

A product move transfers finalization obligations carried by the product or its fields to the new owner.

### Product copying

A product value is copyable only when the product type has a copy contract and every field satisfies the required copy contract.

Copying a product copies every field according to its field type’s copy contract.

Copying a product produces a separate value with its own ownership story.

Copying a product preserves the abstract value according to the product’s copy contract.

Copy behavior is explicit through the product type’s contract.

### Product destruction

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

A product destructor is synchronous and returns `unit`.

Fallible or asynchronous cleanup belongs to finalization.

A product value with finalization obligations must satisfy those obligations before ownership ends, unless the value is transferred to another owner that assumes them or converted into an explicit fallback ownership form.

### Product lifecycle declarations

A product type can define lifecycle declarations inside its type body or inside an inherent implementation for the product type.

```bray
struct File
{
    handle: OsHandle;

    construct(pos path: Path, mode: FileMode = FileMode.read) -> Self
    {
        ...
    }

    construct temp(pos directory: Path, prefix: String = "tmp") -> Self
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

Constructors create fully initialized values of the product type.

A constructor with no name after `construct` is the primary constructor form.

A constructor with a name after `construct` becomes a named constructor under the type:

```bray
let my_file = File.temp(some_path);
```

Finalizers complete required lifecycle obligations before ownership ends.

Destructors perform synchronous cleanup when ownership ends.

Scope enter and exit declarations define scoped capability behavior for `with` expressions.

Lifecycle declarations participate in ownership, borrowing, mutation authority, finalization obligations, effects, and trusted capability checking.

Product lifecycle declarations follow the general lifecycle ordering model.

Product lifecycle declarations use the general lifecycle declaration signature and selection rules.

Product lifecycle declarations are whole-product lifecycle declarations.

`Self` in a product lifecycle declaration means the declaring product type.

In an inherent implementation, the implementation subject must be that product type.

For a given product type, lifecycle kind, and lifecycle path, at most one participating lifecycle declaration can be visible in a
coherence domain.

Constructor bodies have no `self` binding.

A constructor body must produce a fully initialized `Self` value or a `Result.Ok` carrying a fully initialized `Self` value.

A constructor body can produce that value with a product construction expression, another constructor call, or another expression
whose result type is `Self`.

Constructor failure through `Result.Error`, panic, cancellation, or another non-success exit does not produce a product value.

Values, temporaries, and partially initialized product storage created before such an exit are resolved by ordinary ownership,
destruction, and finalization rules.

A successfully constructed product carries:

- the lifecycle obligations declared by the product type,
- the lifecycle obligations of its initialized fields,
- any lifecycle obligations produced by field defaults or constructor body expressions.

Field defaults used during construction are evaluated according to product construction rules before the product becomes fully
initialized.

Finalizer bodies have a compiler-introduced `self` binding for the whole product value being finalized.

The finalizer has exclusive lifecycle authority over `self` for the duration of the finalizer.

A finalizer can observe and mutate fields when its declaration contract permits those operations.

A finalizer cannot let `self`, a field access path, a borrow from `self`, or a capability derived from `self` escape unless the
finalizer contract explicitly transfers the corresponding obligation.

A finalizer must return with the product fully initialized.

If a finalizer returns `Result.Error`, the finalization obligation remains unresolved.

A product with an unresolved finalization obligation cannot be destroyed.

Destructor bodies have a compiler-introduced `self` binding for the whole product value being destroyed.

The destructor has exclusive destruction authority over `self` for the duration of the destructor.

A destructor is synchronous and infallible.

A destructor can observe and mutate fields when its declaration contract permits those operations.

A destructor cannot create a finalization obligation that remains unresolved after the destructor returns.

A destructor cannot let `self`, a field access path, a borrow from `self`, or a capability derived from `self` escape.

If a destructor consumes or destroys a field, that field becomes uninitialized and is not destroyed again after the destructor
returns.

Any initialized fields remaining after the destructor returns are destroyed in product field destruction order.

Scope enter bodies have a compiler-introduced `self` binding for the product access path used as the `with` initializer.

The selected enter declaration must be able to satisfy its declared ownership, borrowing, mutation, capability, effect, trusted,
and lifecycle requirements from that access path.

The successful enter result is the scoped capability matched by the `with` pattern.

The scoped capability can borrow from the product, carry access authority for the product, or carry an independent resource token,
according to the scoped capability type.

Scope exit bodies receive the scoped capability produced by the matching enter declaration.

Exit operates on the scoped capability. It can reach the product only through access carried by that scoped capability.

An active scoped capability can restrict observation, mutation, borrowing, movement, finalization, destruction, and partial moves
of the product for the lifetime of the `with` body.

A whole-product assignment or replacement resolves the old product value according to product finalization, destruction, and field
destruction rules before the new product value becomes initialized at that access path.

### Product layout

The default physical layout of a product type is compiler-defined.

The compiler can choose a layout that preserves Bray semantics.

Field declaration order is part of the semantic representation, but default physical layout is selected by the compiler.

Stable ABI layout, C-compatible layout, packed layout, explicit alignment, explicit representation, and foreign layout belong to explicit layout contracts.

TODO: Define product layout directive syntax.

### Product patterns

Product values can be decomposed by product patterns.

```bray
let { x, y }: Point = point;
```

Product patterns match fields by name.

Field order does not matter.

Field shorthand binds a field to a binding with the same name. The shorthand binding name is not resolved as a named constant or variant.

```bray
{ x, y }
```

`..` explicitly accounts for remaining fields and introduces no bindings.

```bray
{ x, .. }
```

Product patterns are checked by the Pattern Model.

Product patterns participate in ownership, borrowing, copying, partial moves, initialization, destruction, finalization, capability checking, and fact-context refinement according to the pattern operation mode.

### Product API compatibility

For a public product type, public fields are part of the public API.

Adding, removing, renaming, or changing the type of a public field is a public API change.

Changing field mutability is a public API change.

Changing field visibility is a public API change.

Changing field defaults can be a public API change when construction behavior visible to users changes.

Changing lifecycle declarations can be a public API change when ownership, destruction, finalization, construction, or scoped-use behavior changes.

Default physical layout is not public ABI unless the type declares an explicit layout contract.

---

## Union Types

### Union type

A **union type** is a named closed tagged sum type.

A union value has exactly one active variant.

The active variant determines which payload, if any, is initialized.

A union type is declared with `union`.

```bray
union Shape
{
    Circle(center: Point, radius: r64);
    Rectangle(min: Point, max: Point);
    Empty;
}
```

The declared variant set is closed.

The variant set is part of the union type’s definition.

A union value is fully initialized when its active variant tag is initialized and the active variant payload, if any, is fully initialized.

Inactive variant payloads have no initialized values.

### Variant declarations

A variant declaration introduces one variant of a union type.

```bray
union Shape
{
    Circle(center: Point, radius: r64);
    Rectangle(min: Point, max: Point);
    Empty;
}
```

Variant declarations end with semicolons.

The final variant declaration also has a semicolon.

A payload variant uses parentheses.

```bray
Circle(center: Point, radius: r64);
```

A no-payload variant omits parentheses.

```bray
Empty;
```

Payload fields are always named.

Payload field declarations use `:`.

Payload field declarations inside variant parentheses are comma-separated.

Variant names must be unique within the union type.

Payload field names must be unique within the variant payload.

Variant-level visibility modifiers are excluded from union bodies.

A union’s variant surface has the visibility of the union type.

### Variant payload fields

A payload field is a named value stored by a payload variant.

```bray
Circle(center: Point, radius: r64);
```

Each payload field has a name, a type, a mutability contract, an optional default expression, and an initialization state.

Payload fields are immutable by default after initialization.

A payload field declared with `mut` permits post-initialization mutation through a compatible mutable access path when the active variant is known.

```bray
union Shape
{
    Circle(center: Point, mut radius: r64);
    Rectangle(min: Point, max: Point);
}
```

Payload field mutability and binding mutability are separate.

A mutable binding grants mutation authority over the union access path. The payload field declaration still controls whether the reached payload field can be mutated after initialization.

Payload field mutability does not restrict initialization during variant construction.

### Variant payload defaults

A payload field can declare a default expression.

```bray
union Request
{
    Retry(count: i32 = 3, delay: Duration = Duration.seconds(1));
    Cancelled;
}
```

A payload field with a default can be omitted during variant construction.

An omitted defaulted payload field is initialized from its default expression.

A supplied payload field initializer suppresses evaluation of that payload field’s default expression.

A variant payload default is checked in the union declaration context.

A variant payload default cannot reference sibling payload fields.

A variant payload default cannot reference `self`.

A variant payload default is evaluated when that payload field is omitted during construction.

Effects of an evaluated payload default become effects of the variant construction expression.

Finalization obligations created by an evaluated payload default become obligations of the constructed union value, local temporaries, or surrounding context according to ownership and lifecycle rules.

Trusted capabilities used by a payload default must be permitted by the declaration context and construction context according to trusted capability rules.

### Variant contracts

A variant can have contract clauses.

```bray
union Shape
{
    Circle(center: Point, radius: r64)
        requires(
            radius >= 0.0,
        );

    Rectangle(min: Point, max: Point)
        requires(
            min.x <= max.x,
            min.y <= max.y,
        );

    Empty;
}
```

A variant `requires(...)` clause declares facts that must hold when the variant is constructed.

A variant `ensures(...)` clause declares facts established by successful construction of that variant.

Contract clauses use parenthesized comma-separated lists.

Variant contracts are checked during variant construction.

Successful variant construction establishes that the produced union value has the selected active variant.

Successful payload variant construction establishes that the selected payload exists and that its payload fields are initialized.

Successful no-payload variant construction establishes the selected active variant and introduces no payload fields.

### Union construction

Union construction is handled by union variant construction expressions.

```bray
let shape = Shape.Circle(center = origin, radius = 10.0);

let shape: Shape = .Circle(center = origin, radius = 10.0);

let end = ParseResult<i32>.EndOfInput;

let end: ParseResult<i32> = .EndOfInput;
```

Payload variant construction initializes payload fields by name.

Payload field order does not matter.

All non-default payload fields must be initialized.

Omitted defaulted payload fields use their declared defaults.

No-payload variants construct without parentheses.

A leading-dot variant construction expression refers to a variant of the expected union type.

Detailed construction-expression rules belong to the Expression Model.

### Active variant

Every fully initialized union value has one active variant.

The active variant is part of the union value’s runtime state.

Operations that branch on or refine a union value can make the active variant known within a control-flow region.

When the active variant is known, the selected payload exists in that region.

Payload field access requires active-variant refinement proving that the selected payload exists.

A no-payload active variant introduces no payload field access paths.

Active-variant facts participate in the fact context.

Mutation, movement, consumption, destruction, reinitialization, finalization, or capability loss can invalidate active-variant facts when they affect the union value or its storage.

### Union access paths

A union access path reaches the union value.

When the active variant is known, payload field access paths can reach initialized payload fields.

```bray
shape.radius
```

Payload field access requires:

- an access path to the union value,
- active-variant refinement for the selected variant,
- a selected payload field on that variant,
- compatible capability for the requested operation.

Observation of a payload field requires observe capability over the union access path and selected payload.

Mutation of a payload field requires:

- mutation authority over the union access path,
- active-variant refinement,
- a payload field declaration that permits mutation,
- no conflicting active borrows,
- any additional constraints imposed by the payload field type or surrounding context.

Borrowing a payload field creates a borrow of the reached payload field storage.

Moving a payload field out of a union is a partial move of the active payload.

Copying a payload field requires the payload field type to satisfy the copy contract.

### Union replacement

A union value can be replaced as a whole through assignment when the union access path has mutation authority.

```bray
shape = .Rectangle(min = a, max = b);
```

Whole-union replacement ends the old active variant payload according to destruction, finalization, and lifecycle rules.

Whole-union replacement initializes the new active variant tag and payload.

Whole-union replacement changes the active variant fact for the union value.

Facts depending on the previous active variant or old payload are invalidated.

Whole-union replacement depends on mutation authority over the union value, not on payload field mutability.

Payload field mutability controls mutation of payload fields after the active variant is initialized.

### Union initialization state

A union value has initialization state.

The union as a whole can be uninitialized, partially initialized, fully initialized, moved from, or destroyed.

The active variant tag has initialization state.

The active payload, if present, has initialization state.

Each active payload field has its own initialization state while the union is being initialized or after a partial move.

A union value is fully initialized when the active tag is initialized and every field of the active payload is initialized.

Inactive variant payloads have no initialized fields.

A fully initialized union value can be observed, borrowed, moved, copied, consumed, or destroyed as a complete value according to its type and capability state.

A partially initialized union value can be accessed only through initialized active parts when the operation permits partial-state access.

A moved-from union value can be reinitialized when the storage and type contract permit it.

A destroyed union value is no longer usable as a value.

### Partial moves

Moving a payload field out of a union is an ownership operation.

A payload field move requires ownership of the union value.

A payload field move requires no conflicting active borrows of the union value, active payload, or reached payload field.

After a payload field move, the union value is partially initialized.

The moved payload field becomes moved from within the active payload.

Still-initialized active payload fields remain governed by their own ownership and destruction rules.

A partially moved union value can be reinitialized or consumed by a rule that accounts for its partial state.

A partially moved union value can be destroyed as a partial value.

Destruction of a partially moved union value destroys only the still-initialized fields of the active payload.

A partially moved union value can become fully initialized again when all moved-from active payload fields are reinitialized and the storage and type contract permit reinitialization.

A union type with whole-union lifecycle behavior must be fully initialized whenever a whole-union lifecycle declaration can run.

A payload field move from such a union is valid only when every reachable path reinitializes the active payload field before:

- the union is finalized,
- the union is destroyed as a complete value,
- the union is used as a `with` initializer,
- the union is moved, copied, consumed, borrowed, or observed as a complete value,
- the active variant is replaced,
- ownership of the union can end.

If the compiler cannot prove that the union becomes fully initialized before one of those events, the payload field move is
rejected.

Partial union storage resolves only initialized fields of the active payload.

Partial union storage does not run whole-union finalizers, whole-union destructors, or whole-union scope enter/exit behavior.

### Union movement

Moving a fully initialized union value moves the complete union.

Moving the complete union transfers ownership of the active tag and active payload to the new owner.

The old access path becomes moved-from until reinitialized.

Moving a union preserves the union type and active variant.

Moving a union transfers finalization obligations carried by the union or its active payload to the new owner.

### Union copying

A union value is copyable only when the union type has a copy contract and every possible active payload satisfies the required copy contract.

Copying a union copies the active tag.

Copying a union copies the active payload according to the active payload field types’ copy contracts.

Inactive payloads have no initialized values to copy.

Copying a union produces a separate value with its own ownership story.

Copy behavior is explicit through the union type’s contract.

### Union destruction

Destroying a fully initialized union value runs any whole-union destructor before active payload destruction.

The active payload is then destroyed.

Inactive variant payloads have no initialized values and therefore no destruction work.

A no-payload active variant has no payload fields to destroy.

A partially initialized union destroys only initialized fields of the active payload.

A moved-from payload field is not destroyed by the old union owner.

A union type can define a destructor with a `destruct` lifecycle declaration.

A union destructor is synchronous and returns `unit`.

Fallible or asynchronous cleanup belongs to finalization.

A union value with finalization obligations must satisfy those obligations before ownership ends, unless the value is transferred to another owner that assumes them or converted into an explicit fallback ownership form.

### Union lifecycle declarations

A union type can define lifecycle declarations inside its type body or inside an inherent implementation for the union type.

```bray
union ResourceState
{
    Open(handle: OsHandle);
    Closed;

    destruct()
    {
        match self
        {
            case Open(handle)
            {
                ...
            }
            case Closed
            {
                ...
            }
        }
    }
}
```

Note: In the example above, `Open` and `Closed` are ordinary variant names, not special syntax.

Constructors create fully initialized values of the union type.

Finalizers complete required lifecycle obligations before ownership ends.

Destructors perform synchronous cleanup when ownership ends.

Scope enter and exit declarations define scoped capability behavior for `with` expressions.

Lifecycle declarations participate in ownership, borrowing, mutation authority, finalization obligations, effects, and trusted capability checking.

Lifecycle behavior can depend on the active variant.

Union lifecycle declarations follow the general lifecycle ordering model.

Union lifecycle declarations use the general lifecycle declaration signature and selection rules.

Union lifecycle declarations are whole-union lifecycle declarations.

`Self` in a union lifecycle declaration means the declaring union type.

In an inherent implementation, the implementation subject must be that union type.

For a given union type, lifecycle kind, and lifecycle path, at most one participating lifecycle declaration can be visible in a
coherence domain.

Variant-dependent lifecycle behavior is expressed inside whole-union lifecycle bodies with ordinary control flow, pattern matching,
active-variant refinement, and payload access.

Constructor bodies have no `self` binding.

A constructor body must produce a fully initialized `Self` value or a `Result.Ok` carrying a fully initialized `Self` value.

A constructor body can produce that value with a variant construction expression, another constructor call, or another expression
whose result type is `Self`.

Constructor failure through `Result.Error`, panic, cancellation, or another non-success exit does not produce a union value.

Values, temporaries, and partially initialized union storage created before such an exit are resolved by ordinary ownership,
destruction, and finalization rules.

A successfully constructed union carries:

- the lifecycle obligations declared by the union type,
- the lifecycle obligations of the active payload,
- any lifecycle obligations produced by payload defaults or constructor body expressions.

Payload defaults used during construction are evaluated according to union construction rules before the union becomes fully
initialized.

Finalizer bodies have a compiler-introduced `self` binding for the whole union value being finalized.

The finalizer has exclusive lifecycle authority over `self` for the duration of the finalizer.

A finalizer can branch on `self` with ordinary `match`.

A successful variant pattern in a finalizer refines `self` to that active variant and makes initialized payload fields available
according to the pattern operation mode.

A finalizer can observe and mutate active payload fields when its declaration contract permits those operations.

A finalizer cannot let `self`, an active payload access path, a borrow from `self`, or a capability derived from `self` escape
unless the finalizer contract explicitly transfers the corresponding obligation.

A finalizer must return with the union fully initialized.

If a finalizer returns `Result.Error`, the finalization obligation remains unresolved.

A union with an unresolved finalization obligation cannot be destroyed.

Destructor bodies have a compiler-introduced `self` binding for the whole union value being destroyed.

The destructor has exclusive destruction authority over `self` for the duration of the destructor.

A destructor is synchronous and infallible.

A destructor can branch on `self` with ordinary `match`.

A successful variant pattern in a destructor refines `self` to that active variant and makes initialized payload fields available
according to the pattern operation mode.

A destructor can observe and mutate active payload fields when its declaration contract permits those operations.

A destructor cannot create a finalization obligation that remains unresolved after the destructor returns.

A destructor cannot let `self`, an active payload access path, a borrow from `self`, or a capability derived from `self` escape.

If a destructor consumes or destroys an active payload field, that field becomes uninitialized and is not destroyed again after the
destructor returns.

Any initialized active payload fields remaining after the destructor returns are destroyed according to active payload destruction
rules.

Scope enter bodies have a compiler-introduced `self` binding for the union access path used as the `with` initializer.

The selected enter declaration must be able to satisfy its declared ownership, borrowing, mutation, capability, effect, trusted,
and lifecycle requirements from that access path.

The successful enter result is the scoped capability matched by the `with` pattern.

The scoped capability can borrow from the union, carry access authority for the union or its active payload, or carry an
independent resource token, according to the scoped capability type.

Scope exit bodies receive the scoped capability produced by the matching enter declaration.

Exit operates on the scoped capability. It can reach the union only through access carried by that scoped capability.

An active scoped capability can restrict observation, mutation, borrowing, movement, active-variant replacement, finalization,
destruction, and partial moves of the union for the lifetime of the `with` body.

Whole-union replacement resolves the old active variant payload according to union finalization, destruction, and active payload
destruction rules before the new active variant tag and payload become initialized at that access path.

Variant declarations remain the source of variant names and payload structure.

### Recursive unions

A union can be recursive through explicit indirection.

A recursive cycle in a union type must satisfy the recursive stored-field rules.

A recursive payload is valid only when every recursive path back to the declaring union crosses an owning indirection boundary.

```bray
union List<T>
{
    Node(value: T, next: box Self);
    Empty;
}
```

A recursive union that contains itself by value in a cycle without owning indirection is rejected.

```bray
union BadList<T>
{
    Node(value: T, next: Self);
    Empty;
}
```

### Union layout

A union has a semantic active variant tag.

The compiler chooses the default physical tag representation and payload layout.

The default layout can use representation optimizations when Bray semantics are preserved.

Default layout is compiler-defined.

Stable ABI layout, C-compatible layout, explicit tag representation, packed layout, explicit alignment, and foreign layout belong to explicit layout contracts.

TODO: Define union layout directive syntax.

### Union patterns

Union values can be refined and decomposed by variant patterns.

```bray
Circle(center = c, radius = r)
Empty
```

Variant patterns refine the subject to the matched active variant in the matched region.

Payload variant patterns introduce bindings for selected payload fields according to the pattern operation mode.

No-payload variant patterns introduce no payload bindings.

Union patterns are checked by the Pattern Model.

Match expressions over closed unions perform coverage checking against the union’s closed variant set.

Union patterns participate in ownership, borrowing, copying, partial moves, initialization, destruction, finalization, capability checking, and fact-context refinement according to the pattern operation mode.

### Compiler-known result unions

`Result<T, E>` is the compiler-known union type for recoverable domain failure and caught synchronous panic values.

Its semantic declaration is:

```bray
union Result<T, E>
{
    Ok(value: T);
    Error(error: E);
}
```

`Result.Ok` carries the successful value.

`Result.Error` carries the recoverable error value, or the caught panic report when `E` is `PanicReport`.

An uncaught panic is not represented by `Result`.

The caught representation of a synchronous panic is `Result<T, PanicReport>`.

`Result<T, E>` uses ordinary union construction, matching, ownership, movement, borrowing, and coverage rules unless a
language-defined result rule states otherwise.

The `try` expression unwraps `Result.Ok` and propagates `Result.Error` according to result propagation rules.

`RunResult<T>` is the compiler-known union type for observing a task or thread run boundary through `catch`.

Its semantic declaration is:

```bray
union RunResult<T>
{
    Completed(value: T);
    Panicked(report: PanicReport);
    Cancelled;
}
```

`RunResult.Completed` carries the computation's declared result.

`RunResult.Panicked` carries the panic report produced by a panic caught at the task or thread boundary.

`RunResult.Cancelled` records that the task or thread boundary was cancelled before normal completion.

A fallible computation observed through a task or thread boundary uses `RunResult<Result<T, E>>`.

`RunResult<T>` uses ordinary union construction, matching, ownership, movement, borrowing, and coverage rules unless a
language-defined task or thread observation rule states otherwise.

The `try` expression unwraps `RunResult.Completed` and propagates `RunResult.Panicked` or `RunResult.Cancelled` according to
run-result propagation rules.

```bray
match result
{
    case Completed(Ok(value))
    {
        ...
    }

    case Completed(Error(error))
    {
        ...
    }

    case Panicked(report)
    {
        ...
    }

    case Cancelled
    {
        ...
    }
}
```

`PanicReport` is a compiler-known protected-representation type that preserves the panic message and diagnostic context carried by a panic.

`ConversionError` is the compiler-known error type used by built-in checked conversions.

Its semantic declaration is:

```bray
union ConversionError
{
    OutOfRange;
    NonFinite;
    NonRepresentable;
}
```

`ConversionError.OutOfRange` means the source value is outside the target type's value range.

`ConversionError.NonFinite` means the source value is not finite and the selected checked conversion contract requires a finite value.

`ConversionError.NonRepresentable` means the source value is within the target type's range and domain but cannot be represented exactly by the target type without rounding, truncation, saturation, wrapping, or other information loss.

`ConversionError` reports the failure category only.

It does not carry the source value, source type, target type, or source expression location.

Diagnostics can report those details from the conversion expression and type-checking context.

User-defined checked conversions do not use `ConversionError` unless their selected `CheckedConvertTo<Target>.Error` type is `ConversionError`.

### Compiler-known task handles

`Task<T>` is the compiler-known linear task handle type for a spawned asynchronous task whose ordinary result type is `T`.

`Task<T>` is an owned value.

`Task<T>` is not copyable.

Moving a `Task<T>` transfers the task obligation.

Joining or cancelling a `Task<T>` consumes the handle and resolves the task obligation.

The Async Model defines task handle creation, joining, cancellation, transfers, escape rules, borrowing rules, and obligation
checking.

### Union API compatibility

For a public union type, the variant set is part of the public API.

Adding, removing, or renaming a public union variant is a public API change.

Changing a payload field name is a public API change.

Changing a payload field type is a public API change.

Changing payload field mutability is a public API change.

Changing payload defaults can be a public API change when construction behavior visible to users changes.

Changing variant contracts can be a public API change when construction requirements or established facts visible to users change.

Changing lifecycle declarations can be a public API change when ownership, destruction, finalization, construction, or scoped-use behavior changes.

Default physical layout is not public ABI unless the type declares an explicit layout contract.

---

## Type Forms

A **type form** is a syntactic and semantic form that produces a type.

Type forms are compiler-recognized type-level constructs.

A type form defines how a type is built from one or more subject types, compile-time arguments, or structural components.

A type form can affect ownership, storage, borrowing, layout, lifetime behavior, callable behavior, initialization, destruction, finalization, access-path behavior, or value representation.

Type forms are part of the core type grammar.

Examples:

```bray
&T
&mut T
view TraitApplication
box T
box[Heap] T
T?
[T]
[T; N]
(T1, T2)
func(left: T1, right: T2) -> R
```

### Type expressions

A **type expression** is syntax that denotes a type in a type context.

Type expressions do not read runtime storage, create runtime values, or perform runtime evaluation.

Type expressions are composed from:

- type names and qualified type paths,
- generic type applications,
- type-valued member references,
- prefix type forms,
- postfix type forms,
- structural type forms,
- type-form arguments,
- parenthesized type expressions.

Type-form arguments can include types, constants, and other compile-time entities when the type form permits them.

Trait applications can appear inside type expressions only where a type form or qualified type-valued member reference permits them.

Examples:

```bray
Point
geometry.shapes.Circle
List<Point>
Buffer(Iterator<Bytes>).Element
&mut Buffer
box[Heap] Node
Point?
[Point]
[Point; 4]
(Point, Point)
func(buffer: &Buffer, index: usize) -> u8
```

A parenthesized type expression groups a single type expression.

```bray
(box[Heap] Point)?
box[Heap] (Point?)
&mut (box[Heap] Node)
```

Grouping parentheses make type-form composition order explicit.

Grouping parentheses do not produce a new type.

A parenthesized single type expression without a comma is grouping.

```bray
(Point)
```

This denotes `Point`.

Tuple type forms use comma-separated element types.

A one-element tuple type uses a trailing comma.

```bray
(Point,)
```

The trailing comma distinguishes a one-element tuple type from grouping parentheses.

### Subject type

A **subject type** is the type that a type form is applied to.

In:

```bray
box[Heap] Point
```

`Point` is the subject type.

In:

```bray
Point?
```

`Point` is the subject type.

In:

```bray
&mut Buffer
```

`Buffer` is the subject type.

The `view` type form has a trait application subject rather than a subject type.

```bray
view Sink
```

`Sink` is the trait application subject.

Some type forms have one subject type.

Some structural type forms contain multiple subject types.

```bray
(T1, T2)
func(left: T1, right: T2) -> R
```

A type form defines how its subject type or subject types contribute to the produced type.

### Type-form arguments

A type form can accept compile-time arguments.

```bray
box[Heap] T
box[AllocatorStorage<MyAllocator>] T
[T; N]
```

Type-form arguments are part of the produced type.

For example:

```bray
box[Heap] Point
box[ArenaStorage] Point
```

are distinct types because the storage policy argument differs.

A type-form argument is checked in compile-time type-form argument context.

Type-form arguments can include types, constants, and other compile-time entities when the type form permits them.

Each type form defines its permitted argument kinds.

### Prefix type forms

A **prefix type form** appears before its subject type or subject entity.

```bray
&T
&mut T
view TraitApplication
box T
box[Heap] T
```

`&T` is the shared-borrow type form.

`&mut T` is the mutable-borrow type form.

`view TraitApplication` is the trait-view type form.

The `view` type form uses a trait application as its subject entity.

`box T` is the default-storage owned-indirection type form.

`box[S] T` is the owned-indirection type form using storage policy type `S`.

A prefix type form can include compile-time arguments in square brackets.

```bray
box[Heap] List<i32>
box[AllocatorStorage<MyAllocator>] Node
```

Prefix type forms compose with other type forms according to the type grammar.

```bray
&mut box[Heap] Node
box[Heap] Point?
```

Each prefix type form contributes its own ownership, storage, borrowing, lifetime, layout, initialization, destruction, finalization, and capability semantics.

### Postfix type forms

A **postfix type form** appears after its subject type.

```bray
T?
```

`T?` is the nullable type form.

It produces a type whose values and access paths have a nullable storage state.

The absence expression is `none`.

Postfix type forms compose with other type forms according to the type grammar.

```bray
Point?
box[Heap] Point?
```

Each postfix type form contributes its own value-state, ownership, initialization, destruction, finalization, and pattern behavior.

### Structural type forms

A **structural type form** uses a larger syntactic structure to produce a type.

```bray
[T]
[T; N]
(T1, T2)
func(left: T1, right: T2) -> R
```

`[T]` is the unsized slice type form.

`[T; N]` is the fixed-size array type form.

`(T1, T2)` is the tuple type form.

`func(...) -> R` is the callable type form.

Structural type forms can contain one or more subject types and compile-time values.

The structure of the type form is part of the type identity.

### Type-form composition

Type forms compose recursively.

```bray
box[Heap] List<i32>
box[Heap] Point?
&mut box[Heap] Node
box[Heap] view Sink
box[Heap] [u8]
[box[Heap] Node; 4]
func(buffer: &Buffer, index: usize) -> u8
```

The meaning of a composed type is determined by applying each type form according to the type grammar and the semantic contract of that form.

Composition order is determined by the type grammar and explicit grouping parentheses.

A composed type has one ownership, borrowing, initialization, destruction, finalization, capability, effect, and layout contract produced by the composition of its type forms and subject types.

If two composed type forms impose incompatible requirements, the composed type is rejected.

### Borrow type forms

The shared-borrow type form is:

```bray
&T
```

The mutable-borrow type form is:

```bray
&mut T
```

A borrow type is a non-owning access path type.

A borrow value does not own the reached storage.

A borrow has a lifetime.

A borrow is valid only while the reached storage remains valid and the borrow’s capability contract remains satisfied.

A shared borrow allows observation.

A mutable borrow grants temporary exclusive mutation authority over the reached storage.

Nested borrow types are allowed.

```bray
&&T
&&mut T
&mut &T
&mut &mut T
```

Each borrow layer has its own capability.

An outer shared borrow provides shared access to the next layer.

An outer mutable borrow provides mutation authority over the next layer.

The reachable operation depends on the whole access path, including every borrow layer.

TODO: Define detailed borrow rules for borrow type forms.

### Nullable type form

The nullable type form is:

```bray
T?
```

A nullable value or access path has a nullable storage state.

The nullable storage state is either present or absent.

When present, the nullable value contains or reaches a value of type `T`.

When absent, the nullable value contains or reaches no value or storage of type `T`.

The absent state is a valid initialized state.

A nullable value or access path is fully initialized when it is initialized to either present or absent state.

When present, the contained `T` value follows the ownership, borrowing, movement, copying, destruction, finalization, and capability rules of `T`.

When absent, there is no contained `T` value to access, move, copy, borrow, destroy, or finalize.

`T?` is not a named container type.

It does not expose a user-visible variant container or construction wrapper.

The absence expression is:

```bray
none
```

`none` has no standalone type.

`none` is accepted only when the expected type determines a concrete nullable type `T?`.

```bray
let mut count: i32? = none;
count = 10;
count = none;
```

A value of type `T` can initialize or assign the present state of an expected `T?`.

Assigning `none` to a nullable access path changes its nullable storage state to absent.

If the access path owns a present value, that value is destroyed before the access path becomes absent.

If the access path holds a borrow, assigning `none` ends the borrow before the access path becomes absent.

The binding, field, parameter, or other declaration remains declared; only the nullable storage state changes.

Nullable-to-nullable conversion follows the recursive explicit convertibility rule when the contained source type is explicitly convertible to the contained target type.

The absent state remains absent during nullable-to-nullable conversion.

A present value is converted recursively.

Nullable values support two pattern forms:

```bray
none
?pattern
```

`none` matches absent state.

`?pattern` matches present state and applies `pattern` to the contained `T`.

Nullable patterns are the ordinary way to prove present state and bind the contained `T`.

The nullable propagation expression `expression?` evaluates a `T?` expression, produces `T` on the present path, and propagates
`none` from the nearest nullable propagation boundary on the absent path.

### Owned-indirection type form

The owned-indirection type form is:

```bray
box T
box[S] T
```

`box T` uses the default storage policy.

`box[S] T` uses storage policy type `S`.

`T` is the contained subject type.

`S` is the storage policy type.

For sized `T`, the storage policy type must satisfy `Storage<T>`.

The storage policy is an ordinary type that satisfies the compiler-known `Storage<T>` trait for the stored type.

The `Storage<T>` trait is declared by the language substrate and interpreted by the `box` type form.

Implementing `Storage<T>` is ordinary trait implementation plus the trusted declarations required by the storage operations.

The compiler does not infer storage behavior from matching member names.

The compiler recognizes the trait identity of `Storage<T>` and the contracts of its required members.

The core storage contract is:

```bray
trait Storage<T>
{
    trusted static func create(pos value: T) -> Self
        uses(manual_alloc, raw_memory, unchecked_init);

    static func borrow(pos storage: &Self) -> &T;

    static func borrow_mut(pos storage: &mut Self) -> &mut T;

    trusted static func destroy(pos storage: &mut Self)
        uses(raw_memory, unchecked_init);

    trusted static func release(pos storage: Self)
        uses(manual_alloc);
}
```

`Storage<T>.create` creates storage for a fully initialized `T` and moves `value` into that storage.

`Storage<T>.borrow` projects a shared borrow of the stored `T`.

`Storage<T>.borrow_mut` projects a mutable borrow of the stored `T`.

`Storage<T>.destroy` destroys the stored `T` without releasing the storage object itself.

`Storage<T>.release` releases the storage object after the stored value has been destroyed or otherwise removed according to the storage contract.

`Storage<T>.create` requires `T` to be sized.

The `box[S] T` type form can store an unsized subject only when the type-form rule defines how to store a sized concrete value behind that subject.

For `box[S] view TraitApplication`, box construction stores a sized concrete value `U` using `Storage<U>`, then forms the view from the stored `U` and the selected `U(TraitApplication)` implementation witness.

For `box[S] [T]`, box construction uses contiguous owned storage behavior for element type `T` and a runtime element count.

The storage policy allocates storage for the element count, initializes each element exactly once, projects slice borrows, destroys
initialized elements, and releases the allocation according to the storage policy.

```bray
let sink: box[Heap] view Sink = box[Heap](file_sink);
```

Here `file_sink` has a sized concrete type such as `FileSink`.

The storage policy must satisfy `Storage<FileSink>`.

The view type is `view Sink`.

The resulting box owns the stored `FileSink` and carries the implementation witness for `FileSink(Sink)`.

The default storage policy is `Heap`.

A storage policy can require construction arguments.

Those arguments are supplied to the box construction expression after the contained value argument.

```bray
let point: box[AllocatorStorage<MyAllocator>] Point =
    box[AllocatorStorage<MyAllocator>](
        { x = 1.0, y = 2.0, },
        storage = storage,
    );
```

Storage construction arguments are part of the storage policy's construction contract.

They are not part of the `Storage<T>` type application.

A `Storage<T>` implementation must preserve Bray ownership, borrowing, initialization, destruction, finalization, capability, effect, and trusted-obligation rules.

Trusted storage members expose implementation power only inside their bodies.

Calling `box(...)`, borrowing through a box, and destroying a box remain ordinary operations when the selected storage implementation satisfies its public contract.

For sized `T`, a `box[S] T` value owns separately stored `T`.

For `box[S] view TraitApplication`, the box owns the stored concrete value and exposes it through the trait-view subject.

For `box[S] [T]`, the box owns the contiguous element storage and exposes it through the slice subject.

Moving a `box[S] T` moves ownership of the indirection value.

Destroying a `box[S] T` destroys the stored value and releases storage according to the storage policy.

Borrowing a `box[S] T` can project a borrow of the contained or viewed value when the storage policy and access path permit it.

Mutable borrowing a `box[S] T` can project mutable access to the contained or viewed value when the box access path, storage policy, and contained type permit it.

The outer representation of `box[S] T` has statically known finite size independent of `T`.

`box[S] T` can serve as an indirection boundary for recursive types.

Box construction is handled by box construction expressions.

```bray
let node: box List<i32> = box(.Empty);
```

Detailed box construction rules belong to the Expression Model.

### Trait-view type form

The trait-view type form is:

```bray
view TraitApplication
```

`TraitApplication` must be an exact trait application.

If the trait declaration is generic, the view type must supply the generic arguments required by the trait application.

```bray
view Sink
view Encoder<Json>
```

A trait view exposes a hidden concrete value through the callable and contract surface of one trait application.

The concrete implementing type is not part of the visible static type.

A trait view carries the implementation witness needed to dispatch calls through the selected trait implementation.

The runtime representation of a trait view is compiler-defined.

It must preserve the view's ownership, borrowing, lifetime, destruction, finalization, capability, effect, and contract semantics.

`view TraitApplication` is unsized.

It is not a storable value type by itself.

It must appear behind a type form that defines storage or access for an unsized subject.

The defined forms are:

```bray
&view TraitApplication
&mut view TraitApplication
box[S] view TraitApplication
```

This is rejected:

```bray
let sink: view Sink = file_sink;

struct Logger
{
    sink: view Sink;
}
```

This is valid:

```bray
struct Logger
{
    sinks: [box[Heap] view Sink; 4];
}
```

A trait view is not a dynamic type.

It does not permit runtime type tests, downcasting, field access on the hidden concrete type, or calls outside the selected trait view surface.

The only behavior available through a view is behavior declared by the exact trait application and accepted by the view-surface rules.

A concrete value can form a view only when its type satisfies the exact trait application through a participating implementation.

If no participating implementation satisfies the exact trait application, view formation is rejected.

If more than one participating implementation could satisfy the exact trait application, view formation is rejected by coherence rules before the view is formed.

Static functions in a trait are not part of a value view surface.

A callable trait member is part of a view surface only when its signature, contracts, effects, capabilities, and obligations can be checked without naming the hidden concrete type.

A callable trait member that mentions `Self` outside the receiver is not part of a view surface.

A callable trait member with its own generic parameters is not part of a view surface.

A callable trait member that exposes an unfixed type-valued member is not part of a view surface.

A trait with type-valued members can still be used statically through generic constraints and exact trait applications.

When runtime dispatch must expose a related type through a view, that related type must be modeled as an input to the trait application rather than as a type-valued member output.

For example:

```bray
trait Stream<Item>
{
    mut func next() -> Item?;
}

let stream: &mut view Stream<Token> = &mut token_stream;
```

A shared borrowed view permits shared receiver methods.

A mutable borrowed view permits shared and mutable receiver methods.

An owned boxed view permits shared, mutable, and consuming receiver methods according to the box access path, ownership state, and receiver mode.

Consuming a boxed view consumes the owning box value.

The hidden concrete value is destroyed and finalized according to the selected implementation, the concrete type, and the storage policy.

### Fixed-size array type form

The fixed-size array type form is:

```bray
[T; N]
```

`T` is the element type.

`N` is the array length.

`N` is part of the type.

`N` must be greater than zero.

A fixed-size array contains exactly `N` elements of type `T`.

Each element has its own initialization state while the array is being initialized or after a partial move.

An array value is fully initialized when every element is initialized.

Array element access uses indexing syntax.

An array index must be a nonnegative integer index accepted by the array indexing contract.

For an array of length `N`, an element index is valid only when it is less than `N`.

Indexing a fixed-size array access path reaches the selected element access path.

Slicing a fixed-size array access path reaches contiguous substorage with slice type `[T]`.

Array construction is handled by array expressions and array generator expressions.

Moving a complete array moves every initialized element as part of the array value.

Copying an array requires the element type to satisfy the required copy contract.

Borrowing an array borrows the array storage. Element access and slice projection can derive narrower borrows from that borrow
when ordinary borrowing rules permit it.

Moving an element out of an owned array access path is a partial move of the array.

After an element has been moved out, the array is partially initialized.

Destruction of a partially initialized array destroys only initialized elements.

Array destruction processes initialized elements in reverse index order.

Finalization obligations retained by array elements are retained by the array.

Detailed array expression rules belong to the Expression Model.

Detailed array indexing and slicing expression rules belong to the Expression Model.

### Slice type form

The slice type form is:

```bray
[T]
```

`T` is the element type.

`[T]` is an unsized contiguous sequence type.

The slice length is runtime state carried by an indirection boundary.

A slice type cannot appear as a local value by itself, a by-value parameter type, a by-value result type, or a by-value field type.

A slice type can appear behind an indirection boundary:

```bray
&[T]
&mut [T]
box[S] [T]
```

`&[T]` is a shared slice borrow.

`&mut [T]` is a mutable slice borrow.

`box[S] [T]` is owned contiguous slice storage using storage policy `S`.

Owned slice storage has a fixed length after construction.

Resizable buffers are library types built on top of storage primitives rather than a meaning of `[T]`.

A shared slice borrow permits observation of initialized elements.

A mutable slice borrow permits mutation of initialized elements according to ordinary exclusive-borrow rules.

Borrowed slices do not own their elements.

Moving an element out through a borrowed slice is not allowed.

Moving `box[S] [T]` moves the owned contiguous storage, its runtime length, and its initialized elements as one owned value.

Destroying `box[S] [T]` destroys initialized elements in reverse index order and then releases the underlying storage through `S`.

Borrowing `box[S] [T]` can produce `&[T]` or `&mut [T]` according to the borrowing mode and access path authority.

Indexing `box[S] [T]` projects through the owned indirection to the selected element.

Slicing `box[S] [T]` projects through the owned indirection to contiguous substorage.

Slice projection never copies elements.

Slice projection cannot change the length of the projected storage.

### Tuple type form

A tuple type form is:

```bray
(T1, T2)
```

A tuple type is a fixed-size ordered product type.

The arity of a tuple is the number of element types.

The element types are part of the tuple type in order.

A one-element tuple type uses a trailing comma.

```bray
(T,)
```

The trailing comma distinguishes a one-element tuple type from grouping parentheses.

Each tuple element has its own type and initialization state.

A tuple value is fully initialized when every element is initialized.

Tuple construction is handled by tuple expressions.

Moving a complete tuple moves every initialized element as part of the tuple value.

Copying a tuple requires every element type to satisfy the required copy contract.

Borrowing a tuple borrows the tuple storage. Tuple element projection can derive narrower borrows from that borrow when ordinary
borrowing rules permit it.

Moving an element out of an owned tuple access path is a partial move of the tuple.

After an element has been moved out, the tuple is partially initialized.

Destruction of a partially initialized tuple destroys only initialized elements.

Tuple destruction processes initialized elements in reverse element order.

Finalization obligations retained by tuple elements are retained by the tuple.

Detailed tuple expression rules belong to the Expression Model.

Tuple element projection uses dot-number syntax.

```bray
pair.0
pair.1
```

The projection number is a compile-time tuple element position.

The selected position must be within the tuple arity.

Tuple element projection reaches the selected element access path.

Tuples do not support bracket indexing or slicing.

### Callable type form

The callable type form is:

```bray
func(parameter: Type, ...) -> Result
```

A callable type describes a callable value’s parameter names, parameter call-position permissions, parameter types, result type, execution mode, ownership behavior, borrowing behavior, mutation requirements, lifetime requirements, capability requirements, effects, trusted caller obligations, and finalization behavior.

Callable parameter names and `pos` permissions are part of the callable contract because they define how call arguments bind to parameters.

```bray
func(left: i32, right: i32) -> i32
func(pos value: i32) -> i32
async func(pos request: Request) -> Response
```

A callable returning `unit` can omit the result type.

Callable types preserve caller-visible obligations.

A callable with trusted caller obligations requires a callable type that preserves those obligations.

Callable values and callable declarations are checked by the Function and Callable Model.

Contract clauses attach after the callable type signature.

```bray
func(pos value: i32) -> i32
    requires(value >= 0)
```

Contract clauses on callable type forms use the same predicate-expression syntax as contract clauses on callable declarations.

Async callable type forms use `async` before `func`.

```bray
async func(pos request: Request) -> Response
```

Trusted callable type forms preserve trusted callable obligations and trusted implementation capability requirements through their
callable contract.

```bray
trusted func(pos bytes: &mut [u8])
    uses(raw_memory)
```

A named callable contract declaration gives a reusable name to a callable type form.

```bray
callable Transform =
    func(pos value: i32) -> i32;
```

The callable name can be used in type positions that expect a callable type.

A named callable contract can be generic.

```bray
callable Mapper<T, U> =
    func(pos value: T) -> U;
```

A named callable contract can have `with(...)` constraints.

```bray
callable OrderedPredicate<T>
    with(T: Comparable<T>) =
    func(pos left: &T, pos right: &T) -> bool;
```

A named callable contract is not a general type alias.

It can name only callable type forms.

It does not rename arbitrary type expressions, functions, methods, lambdas, implementations, modules, or packages.

A callable value satisfies a named callable contract when its visible callable contract satisfies the named contract.

Named callable contracts cannot be overloaded.

Lambda expressions produce anonymous callable values.

A lambda's callable type is described by its parameter names, parameter call-position permissions, parameter types, result type,
execution mode, contract clauses, effect clauses, capability clauses, trusted obligations, and captured state.

Capture state is not a callable parameter.

Capture state is part of the callable value's ownership, borrowing, initialization, destruction, finalization, capability, effect,
lifetime, and storage contract.

A capture-free lambda can be used where an expected callable type accepts a callable value with the same visible callable contract.

A capture-bearing lambda can be used where the expected callable type preserves the captured state's ownership, borrowing,
capability, effect, lifetime, and finalization obligations.

Two lambda expressions with the same visible callable signature can still produce distinct callable value types when their captured
state differs.

The call surface of a lambda is its visible callable contract.

The representation of a lambda's captured state is not part of the callable parameter list and is not directly accessible through
the callable value.

### Type forms and construction

Some type forms define construction expression syntax.

`box` defines a type-form construction expression.

```bray
box[S] T
box[S](value, ...)
```

A type form with construction behavior defines how a value of the produced type is constructed from runtime expressions and compile-time arguments.

A type-form construction expression is compiler-recognized.

A type-form construction expression can invoke ordinary declarations, trait behavior, lifecycle declarations, storage behavior, trusted declarations, and contract clauses according to the type form’s rules.

A type form with no construction behavior cannot be used as a construction expression through type-form syntax.

Detailed type-form construction rules belong to the Expression Model.

### Type forms and recursive types

A stored field type must have known finite outer size.

A recursively reachable stored field is valid only when every recursive path back to the declaring type crosses an owning indirection boundary.

An **owning indirection boundary** is a type form whose outer value has known finite size and owns storage for its subject separately from the outer value.

The currently defined owning indirection boundary is `box[S] T`.

This includes `box[S] view TraitApplication`.

```bray
struct Node
{
    next: Self; // invalid
}

struct Node
{
    next: box[Heap] Self; // valid
}
```

Borrow types are finite access values.

Borrow types are non-owning and do not make a type own recursive structure.

```bray
struct View
{
    next: &Self; // valid finite representation, non-owning
}
```

Structural type forms do not break recursive stored ownership.

Products, union payloads, tuples, arrays, and nullable values keep their stored subjects inline for recursive sizing.

```bray
struct Node
{
    next: Self?; // invalid
}

struct Node
{
    children: [Self; 2]; // invalid
}

struct Node
{
    next: (box[Heap] Self)?; // valid
}
```

Generic type applications do not hide recursive storage.

After generic substitution, the resulting stored field type must satisfy the same recursive stored-field rules.

```bray
struct Wrap<T>
{
    value: T;
}

struct Node
{
    next: Wrap<Self>; // invalid
}
```

### Type forms and layout

A type form contributes layout constraints to the produced type.

Some type forms have compiler-defined default layout.

Some type forms can use representation optimizations when their semantics are preserved.

Stable ABI layout, C-compatible layout, explicit representation, explicit alignment, packed layout, and foreign layout belong to explicit layout contracts.

TODO: Define layout directive syntax and layout contracts.

### Type-form eligibility

A type form is introduced by the language when the form has core semantic meaning.

A type form earns core status when ordinary named types and behavioral contracts cannot express the construct without losing required compiler knowledge about ownership, storage, borrowing, layout, lifetime behavior, callable behavior, initialization, destruction, or finalization.

`box` is a type form because owned indirection affects recursive type sizing, ownership transfer, destruction, borrow projection, and storage identity.

`?` is a type form because nullability is a core value-state shape used throughout the language.

`func(...) -> ...` is a type form because callable values carry parameter, result, ownership, effect, execution, and contract semantics.

---

## Traits

A **trait** is a named behavioral contract.

A trait defines behavior that a type can satisfy through an explicit implementation.

Traits are declared with `trait`.

```bray
trait Equatable<Other>
{
    func equals(pos other: &Other) -> bool;
}
```

A trait declaration introduces a named compile-time entity.

A trait is used by implementations, constraints, method resolution, callable checking, and public API compatibility.

A trait can be generic.

```bray
trait Comparable<Other>
{
    func compare(pos other: &Other) -> Ordering;
}
```

A generic trait produces trait applications when supplied with generic arguments.

```bray
Comparable<Point>
```

A trait application is the trait plus its supplied arguments.

A trait application is part of the behavioral contract that an implementation satisfies.

### Trait visibility

A trait declaration can be `public` or `internal`.

`public` is the default.

```bray
trait Equatable<Other>
{
    func equals(pos other: &Other) -> bool;
}

internal trait ParserDiagnostics
{
    func report_state() -> ParserState;
}
```

A public trait exposes its full contract surface as public API.

An internal trait is available within its intended scope.

Use of an internal trait outside its intended scope requires explicit internal-use acknowledgement.

Trait members inherit the visibility of the trait.

The grammar excludes `public` and `internal` modifiers on individual trait members.

```bray
trait Equatable<Other>
{
    func equals(pos other: &Other) -> bool;
}
```

The full trait body is the contract surface of the trait.

### Trait member declarations

A trait body contains member declarations that make up the trait’s behavioral contract.

The currently defined trait member forms are:

- callable member declarations,
- type-valued member declarations.

```bray
trait Equatable<Other>
{
    func equals(pos other: &Other) -> bool;
}
```

A trait callable member can be required or defaulted.

A required callable trait member has no body and ends with a semicolon.

```bray
trait Equatable<Other>
{
    func equals(pos other: &Other) -> bool;
}
```

A defaulted callable trait member has a body.

```bray
trait Equatable<Other>
{
    func equals(pos other: &Other) -> bool;

    func not_equals(pos other: &Other) -> bool
    {
        ...
    }
}
```

A defaulted member body provides default behavior for implementations that do not supply that member.

A defaulted member body is checked in trait context.

A defaulted member body can use the trait’s declared surface, `self` when the member is an instance method, `Self`, trait parameters, type-valued members, available constraints, and declarations visible from the trait declaration context.

A defaulted member body must satisfy the member’s declared result type, ownership behavior, borrowing behavior, capability contract, effect contract, and contract clauses.

TODO: Define additional trait member kinds such as constants, predicates, and lifecycle requirements.

### Type-valued members in traits

A type-valued member is a type-level output of a trait implementation.

Type-valued members are allowed only in trait declarations.

```bray
trait Iterator
{
    type Element;

    mut func next() -> Element?;
}
```

A type-valued member declaration without a binding introduces a required type member.

Inside the declaring trait, the type-valued member name is available in that trait’s member signatures, default bodies, and contract clauses.

Type-valued members are immutable.

A type-valued member cannot be declared `mut`.

A type-valued member cannot be rebound after the implementation has selected its value.

A type-valued member is not a type alias.

A type-valued member does not introduce an alternate name for an arbitrary type outside the trait relationship that defines it.

Module-level type aliases are not part of Bray.

Structs, unions, modules, packages, functions, and inherent implementations cannot declare type-valued members.

Trait parameters and type-valued members have different roles:

- trait parameters are inputs to a trait application,
- type-valued members are outputs selected by the implementation of a trait application.

For a given participating implementation of a concrete trait application, every type-valued member has exactly one selected type.

The selected type can depend on the implementing type and on the trait application’s generic arguments.

The selected type cannot depend on a runtime value, control-flow path, local inference choice, caller preference, or use site.

An implementation of a trait with required type-valued members must bind each required type member explicitly.

```bray
impl TokenCursor(Iterator)
{
    type Element = Token;

    mut func next() -> Element?
    {
        ...
    }
}
```

The `type Element = Token;` implementation member is a trait-member binding.

It is not a general type alias declaration.

Inside an implementation of the trait, the type-valued member name refers to the selected type bound by that implementation.

An implementation member that refers to a type-valued member is checked after substituting the implementation’s selected type.

A trait implementation satisfies the trait only when its callable members match the trait contract after all type-valued member bindings are applied.

Type-valued members participate in type checking, callable checking, method resolution, generic constraints, contract checking, fact-context checking, documentation, and public API compatibility.

Type-valued members do not create runtime type identity.

Type-valued members do not permit downcasting, runtime type tests, or dynamic type mutation.

Dynamic dispatch through a trait view is rejected when it would hide selected type-valued members that are visible through that dispatch surface.

Type-valued members cannot have their own generic parameters.

```bray
trait StreamingParser
{
    type Output;       // valid
    type Token<T>;     // invalid
}
```

Type-valued member defaults are not allowed.

```bray
trait Parser
{
    type Error = ParseError; // invalid
}
```

Changing a type-valued member binding can change the public contract of the implementation.

Outside the declaring trait and an implementation of that trait, a type-valued member is referenced with a qualified type-valued member reference:

```bray
SubjectType(TraitApplication).MemberName
```

The syntax mirrors trait implementation syntax.

`SubjectType` is the implementing type whose selected member type is being referenced.

`TraitApplication` is the trait application that declares the type-valued member.

`MemberName` is the type-valued member declared by that trait.

For example:

```bray
TokenCursor(Iterator).Element
```

A generic type parameter can be the subject when the surrounding constraints require the trait application:

```bray
func first<I>(iter: I) -> I(Iterator).Element?
    with(I: Iterator)
{
    return iter.next();
}
```

The result type is the `Element` selected by the participating implementation of `Iterator` for `I`.

Trait applications with generic arguments are written inside the parentheses:

```bray
Buffer(JsonEncode<Compact>).Output
Buffer(JsonEncode<Pretty>).Output
```

The trait application in a qualified type-valued member reference must be exact.

An implementation overload family header is not an exact trait application reference when it names several applications.

```bray
Buffer(Iterator<Bytes>).Element
Buffer(Iterator<Lines>).Element
```

The trait application is required outside the declaring trait and its implementation.

```bray
I.Element             // invalid
I(Iterator).Element   // valid
```

The unqualified member name is available only inside the declaring trait and inside an implementation of that trait.

A qualified type-valued member reference is a type expression.

It does not access a runtime field or value member.

The qualified reference is valid only when the referenced implementation is available in the checking context.

If no matching implementation is available, the reference is rejected.

If more than one matching implementation is available in the relevant coherence domain, the reference is rejected as ambiguous.

Visibility and internal-use acknowledgement rules apply to the trait application, implementation, and selected type.

### Instance methods in traits

Inside a trait body, `func` declares an instance method by default.

```bray
trait Sized
{
    func length() -> usize;
}
```

An instance method has an implicit receiver.

The receiver is not written as a parameter.

The keyword `self` refers to the current receiver inside an instance method body.

The keyword `Self` refers to the implementing type inside a trait declaration and inside implementations of that trait.

```bray
trait Cloneable
{
    func clone() -> Self;
}
```

The grammar reserves `self` for the current receiver in instance method bodies.

`self` cannot be declared as an ordinary parameter, local binding, or pattern binding.

An instance method’s ordinary parameters are written inside the parameter list.

Ordinary parameters may still use `Self` as a type when `Self` is in scope.

```bray
trait Comparable<Other>
{
    func compare(pos other: &Other) -> Ordering;
}
```

The receiver is supplied by method-call syntax.

```bray
point.compare(other)
```

Parameters are named by default.

Parameters marked `pos` can be supplied positionally.

### Receiver modes

The method declaration form determines the receiver mode.

```bray
func length() -> usize;

mut func clear();

consume func into_bytes() -> Bytes;

consume mut func normalize() -> Self;
```

`func` declares an instance method with a shared receiver.

A shared receiver method can observe the receiver.

`mut func` declares an instance method with a mutable receiver.

A mutable receiver method requires mutation authority over the receiver access path when called.

`consume func` declares an instance method that consumes the receiver.

A consuming receiver method requires ownership of the receiver value when called.

After a consuming receiver method call, the old receiver access path is unavailable until reinitialized.

`consume mut func` declares an instance method that consumes the receiver and gives the method body mutable local authority over `self`.

The receiver mode is part of the member’s callable contract.

An implementation member must use the same receiver mode as the trait member it fulfills.

### Static functions in traits

A static trait function is declared with `static func`.

```bray
trait Parse<T>
{
    static func parse(pos text: String) -> T;
}
```

A static function has no receiver.

`self` is unavailable inside a static function body.

`Self` is available in the static function signature and body.

```bray
trait Empty
{
    static func empty() -> Self;
}
```

Static functions are type-level behavior.

Static functions are opt-in with the `static` modifier because trait callable members default to instance methods.

### Trait contracts

Trait member declarations can have callable contracts.

```bray
trait Comparable<Other>
{
    func compare(pos other: &Other) -> Ordering
        requires(
            ...
        )
        ensures(
            ...
        );
}
```

`requires(...)` declares preconditions for the member.

`ensures(...)` declares postconditions for the member.

Contract clauses use parenthesized comma-separated lists.

A required trait member’s contract is part of the obligation that implementations must satisfy.

A defaulted trait member’s body is checked against its contract.

An implementation member must satisfy the contract of the trait member it fulfills.

Rules for predicate expressions, fact contexts, trusted obligations, and contract clauses belong to the Contract and Trust Model.

A required trait member can be trusted.

A trusted required trait member uses the `trusted` modifier and a `uses(...)` clause.

The `uses(...)` clause on a required trait member declares the trusted implementation capability envelope for that member.

An implementation member that fulfills a trusted required trait member must also be trusted.

The implementation member's `uses(...)` clause must be a subset of the required member's capability envelope.

The implementation member's `uses(...)` clause must still exactly match the trusted capabilities used by that implementation body.

A trusted required trait member does not by itself impose a trusted caller obligation.

Trusted caller obligations must be declared with `trusted` requirements in `requires(...)` or another caller-visible contract clause.

TODO: Define effect annotation syntax beyond currently defined contract clauses.

### Conversion traits

Conversion expression syntax is implemented through compiler-known traits.

The language substrate declares the conversion trait surface.

A type enables a conversion by satisfying the corresponding compiler-known trait application.

Source code does not declare new conversion modes or implicit conversion behavior.

Plain conversion uses `ConvertTo<Target>`:

```bray
trait ConvertTo<Target>
{
    consume func convert() -> Target;
}
```

`ConvertTo<Target>` is the compiler-known trait application for `as Target`.

`convert` is the member called by a plain conversion expression when no built-in recursive conversion rule applies.

A `ConvertTo<Target>` implementation must be total and value-preserving according to the conversion contract.

Fallible conversion uses `CheckedConvertTo<Target>`:

```bray
trait CheckedConvertTo<Target>
{
    type Error;

    consume func convert_checked() -> Result<Target, Error>;
}
```

`CheckedConvertTo<Target>` is the compiler-known trait application for `as checked Target`.

`convert_checked` is the member called by a checked conversion expression when no built-in checked conversion rule applies.

The selected `Error` type becomes the error type of the checked conversion result.

An implementation of a conversion trait is an ordinary trait implementation:

```bray
impl PortToU16 = Port(ConvertTo<u16>)
{
    consume func convert() -> u16
    {
        return self.value;
    }
}

impl StringToPort = String(CheckedConvertTo<Port>)
{
    type Error = ParseError;

    consume func convert_checked() -> Result<Port, Error>
    {
        ...
    }
}
```

Conversion trait implementations follow trait implementation coherence.

The exact coherence key is still:

```text
(ImplementingType, TraitApplication)
```

For `as Target`, the trait application includes the target type:

```text
(SourceType, ConvertTo<Target>)
```

For `as checked Target`, the trait application includes the target type:

```text
(SourceType, CheckedConvertTo<Target>)
```

If the relevant implementation belongs to an implementation overload family, conversion expression resolution uses the same implementation overload rules as method calls.

The source type, target type, conversion mode, and compiler-known conversion member name can select an implementation arm.

Result type, expected type, and type-valued member outputs do not select a conversion implementation.

No ranking is performed between conversion implementation candidates.

If no participating implementation matches, the conversion expression is rejected.

If more than one participating implementation remains possible, the conversion expression is rejected as ambiguous.

Conversion expressions can use only the public compiler-known conversion trait surface and public participating implementations in the current coherence domain.

Internal-access acknowledgement does not make an implementation candidate available to a conversion expression.

Internal conversion behavior can be exposed through named functions or methods when the caller explicitly opts into the internal declaration according to the internal-access rules.

Conversion trait members consume the receiver value produced by the source expression.

When the source expression produces a borrow value, consuming the borrow value does not consume the borrowed storage.

### Operator traits

Operator syntax is implemented through compiler-known traits.

The language substrate declares the overloadable operator set as part of the compiler-known trait surface.

A type enables an operator by satisfying the corresponding compiler-known trait application.

Source code does not declare new operator symbols, new operator precedence, or mappings from arbitrary functions to operator tokens.

The overloadable operators are:

| Operator form | Trait application     | Member            | Result            |
|---------------|-----------------------|-------------------|-------------------|
| infix `+`     | `Add<Rhs>`            | `add`             | selected `Output` |
| infix `-`     | `Subtract<Rhs>`       | `subtract`        | selected `Output` |
| infix `*`     | `Multiply<Rhs>`       | `multiply`        | selected `Output` |
| infix `/`     | `Divide<Rhs>`         | `divide`          | selected `Output` |
| infix `%`     | `Remainder<Rhs>`      | `remainder`       | selected `Output` |
| infix `**`    | `Exponentiate<Rhs>`   | `exponentiate`    | selected `Output` |
| infix `@`     | `MatrixMultiply<Rhs>` | `matrix_multiply` | selected `Output` |
| prefix `-`    | `Negate`              | `negate`          | selected `Output` |
| infix `==`    | `Equatable<Rhs>`      | `equals`          | `bool`            |
| infix `!=`    | `Equatable<Rhs>`      | `equals`          | `bool`            |
| infix `<`     | `Comparable<Rhs>`     | `compare`         | `bool`            |
| infix `<=`    | `Comparable<Rhs>`     | `compare`         | `bool`            |
| infix `>`     | `Comparable<Rhs>`     | `compare`         | `bool`            |
| infix `>=`    | `Comparable<Rhs>`     | `compare`         | `bool`            |
| infix `&`     | `BitAnd<Rhs>`         | `bit_and`         | selected `Output` |
| infix `\|`    | `BitOr<Rhs>`          | `bit_or`          | selected `Output` |
| infix `^`     | `BitXor<Rhs>`         | `bit_xor`         | selected `Output` |
| prefix `~`    | `BitNot`              | `bit_not`         | selected `Output` |
| infix `<<`    | `ShiftLeft<Rhs>`      | `shift_left`      | selected `Output` |
| infix `>>`    | `ShiftRight<Rhs>`     | `shift_right`     | selected `Output` |

The value-producing binary operator traits have this shape:

```bray
trait Add<Rhs>
{
    type Output;

    func add(pos rhs: &Rhs) -> Output;
}
```

`Add<Rhs>` is the compiler-known trait application for binary `+`.

`add` is the member called by the `+` operator.

The other value-producing binary operator traits use the same shared-receiver shape and differ only by trait name, member name, operator token, and operator meaning.

The unary value-producing operator traits have this shape:

```bray
trait Negate
{
    type Output;

    func negate() -> Output;
}
```

`BitNot` uses the same shape with `bit_not`.

Equality uses `Equatable<Rhs>`:

```bray
trait Equatable<Rhs>
{
    func equals(pos rhs: &Rhs) -> bool;
}
```

`left == right` calls `equals`.

`left != right` is derived from the boolean inverse of `equals`.

`!=` is not implemented separately.

Relational comparison uses `Comparable<Rhs>`:

```bray
trait Comparable<Rhs>
{
    func compare(pos rhs: &Rhs) -> Ordering;
}
```

`<`, `<=`, `>`, and `>=` are derived from the returned `Ordering`.

The relational operators are not implemented separately.

`MatrixMultiply<Rhs>` is the compiler-known trait for `@`.

`@` is reserved for linear-algebra multiplication semantics, including vector dot product, matrix-vector multiplication, and matrix-matrix multiplication.

The operator token, fixity, arity, precedence, associativity, trait name, member name, receiver mode, operand parameter contract, and result rule are part of the compiler-known trait contract.

An implementation of an operator trait is an ordinary trait implementation:

```bray
impl Vec2Add = Vec2(Add<Vec2>)
{
    type Output = Vec2;

    func add(pos rhs: &Vec2) -> Output
    {
        return Vec2 { x = self.x + rhs.x, y = self.y + rhs.y };
    }
}
```

Given participating implementations for the operand types, `left + right` resolves to the `Add<RightType>.add` member for `LeftType`.

Operator trait members are public language surface.

A unary or binary expression using an overloadable token can use only the public compiler-known operator trait surface and public participating implementations in the current coherence domain.

Unary and binary expressions using overloadable tokens do not opt into internal access.

Internal-access acknowledgement does not make an implementation candidate available to a unary or binary expression.

Internal implementation details can be used from the named member body, but they are not exposed by the expression itself.

An operator trait member cannot require mutation authority over the receiver or over caller-provided operand storage.

For receiver-based operators, the operator member uses the shared receiver mode.

An operator trait member cannot be declared with `mut func`, `consume func`, or `consume mut func`.

An explicit operand parameter receives a shared borrow according to the compiler-known trait signature.

An explicit operand parameter cannot require a mutable borrow.

A unary or binary expression using an overloadable token does not consume either operand.

If an operation needs to consume an operand or mutate caller-provided storage, it is expressed as a named method or function rather than an operator.

Operators not listed in the overloadable operator table are not overloadable.

Assignment, compound assignment, field access, method calls, function calls, indexing, slicing, ranges, borrowing, nullable propagation, result propagation, panic catching, awaiting, spawning, construction forms, pattern matching, and lifecycle forms are not operator-overload hooks.

Unary and binary expressions using overloadable tokens follow trait implementation coherence.

The exact coherence key is still:

```text
(ImplementingType, TraitApplication)
```

For binary `+`, the trait application includes the right operand type:

```text
(LeftType, Add<RightType>)
```

If the relevant implementation belongs to an implementation overload family, overloadable token resolution uses the same implementation overload rules as method calls.

Operand types, receiver compatibility, and the operator trait member name can select an implementation arm.

Result type, expected type, and type-valued member outputs do not select an operator implementation.

No ranking is performed between operator implementation candidates.

If no participating implementation matches, the unary or binary expression is rejected.

If more than one participating implementation remains possible, the unary or binary expression is rejected as ambiguous.

## Implementations

An **implementation** declares behavior for a type.

Implementations are declared with `impl`.

An inherent implementation is written as `impl Type`.

```bray
impl Point
{
    func distance_to(pos other: &Self) -> r64
    {
        ...
    }

    static func origin() -> Point
    {
        ...
    }
}
```

An inherent implementation adds behavior associated with the type.

An inherent implementation does not add fields to the type’s primary representation.

A trait implementation can be unnamed or named.

An unnamed trait implementation is written as `impl Type(TraitApplication)`.

```bray
impl Point(Equatable<Point>)
{
    func equals(pos other: &Self) -> bool
    {
        ...
    }
}
```

A named trait implementation is written as `impl ImplementationName = Type(TraitApplication)`.

```bray
impl PointEquatable = Point(Equatable<Point>)
{
    func equals(pos other: &Self) -> bool
    {
        ...
    }
}
```

`ImplementationName` is the implementation identity.

It is not a type, a type alias, or a wrapper around the subject type.

The subject type before the parentheses determines `Self`, the receiver type, and the storage being implemented for.

The trait application inside the parentheses determines the contract being fulfilled.

For a generic trait application:

```bray
impl Point(Comparable<Point>)
{
    func compare(pos other: &Point) -> Ordering
    {
        ...
    }
}
```

For a generic implementing type:

```bray
impl BufferEquatable = Buffer<T>(Equatable<Buffer<T>>)
{
    func equals(pos other: &Self) -> bool
    {
        ...
    }
}
```

A trait implementation makes the implementing type satisfy the specified trait application.

Trait satisfaction is explicit.

A type satisfies a trait application through an accepted participating implementation declaration for that exact subject type and trait application.

Matching member names and signatures alone gives no trait satisfaction.

### Implementation members

An implementation body contains member definitions.

In an inherent implementation, member definitions become behavior associated with the implementing type.

In a trait implementation, member definitions fulfill members of the implemented trait application.

```bray
impl Point(Equatable<Point>)
{
    func equals(pos other: &Self) -> bool
    {
        ...
    }
}
```

A trait implementation must provide every required callable trait member that has no default body.

A trait implementation must bind every required type-valued member.

A trait implementation can provide a member that has default behavior in the trait.

When an implementation provides a member with default behavior, the implementation member is used for that implementation.

When an implementation omits a member with default behavior, the trait’s default behavior is used for that implementation.

An implementation callable member must match the fulfilled trait member’s name, receiver mode, parameter names, parameter types, result type, execution mode, contract obligations, and caller-visible effects after type-valued member bindings have been applied.

An implementation type-valued member binding must match a type-valued member declared by the implemented trait.

An implementation type-valued member binding must select a concrete type that is valid in the implementation context.

An implementation cannot bind the same type-valued member more than once.

An implementation cannot provide extra type-valued member bindings that are not declared by the trait.

Implementation member visibility is governed by the implementation relationship and the implemented trait or inherent implementation context.

The grammar excludes `public` and `internal` modifiers on individual trait-implementation members.

Implementation members in a trait implementation are fulfillments of a trait contract, not independent visibility surfaces.

### `Self` and `self`

`Self` is the implementing type in trait and implementation contexts.

Inside:

```bray
trait Cloneable
{
    func clone() -> Self;
}
```

`Self` means the type that implements `Cloneable`.

Inside:

```bray
impl Point(Cloneable)
{
    func clone() -> Self
    {
        ...
    }
}
```

`Self` means `Point`.

`self` is the current receiver inside an instance method body.

```bray
impl Point
{
    func distance_to(pos other: &Self) -> r64
    {
        ...
    }
}
```

The receiver is supplied by method-call syntax.

```bray
point.distance_to(other)
```

`self` is available only inside instance method bodies.

The binding name `self` is reserved for the compiler-introduced receiver.

Static function bodies use `Self` for the implementing type and receive ordinary parameters through their parameter list.

### Generic traits

A trait can have generic parameters.

```bray
trait Comparable<Other>
{
    func compare(pos other: &Other) -> Ordering;
}
```

A trait implementation supplies a concrete trait application.

```bray
impl Point(Comparable<Point>)
{
    func compare(pos other: &Point) -> Ordering
    {
        ...
    }
}
```

A generic implementation can satisfy a parameterized set of trait applications.

```bray
impl BufferComparable = Buffer<T>(Comparable<Buffer<T>>)
    with(T: Comparable<T>)
{
    func compare(pos other: &Buffer<T>) -> Ordering
    {
        ...
    }
}
```

Generic implementation parameters are inferred from the subject type and trait application.

```bray
impl BufferComparable = Buffer<T>(Comparable<Buffer<T>>)
```

An otherwise unresolved generic name that appears in the subject type or trait application becomes an implementation parameter.

If a name resolves to an existing type, constant, capability, effect, lifetime, or other visible declaration, it is not inferred as an implementation parameter.

The implementation name is written without a generic parameter list.

The subject type, trait application, `with(...)` clause, and implementation body can use inferred implementation parameters.

The `with(...)` clause can constrain inferred implementation parameters.

The `with(...)` clause cannot introduce implementation parameters by itself.

Every inferred implementation parameter must appear in the subject type or trait application.

This is rejected:

```bray
impl BadCloneBuffer = Buffer<i32>(Cloneable)
    with(T: Cloneable)
{
    ...
}
```

`T` appears only in the `with(...)` clause, so it is not an implementation parameter.

A generic implementation represents a parameterized set of exact trait implementations.

For each valid substitution of the implementation parameters, the implementation produces one exact coherence key:

```text
(SubstitutedSubjectType, SubstitutedTraitApplication)
```

A substitution is valid only when it satisfies the implementation’s `with(...)` clause and makes the subject type and trait application well formed.

The implementation body is checked once under the implementation’s static constraints.

The body can use only operations, type-valued members, constants, effects, capabilities, and facts established by the implementation’s `with(...)` clause and surrounding declaration context.

Generic implementation overlap is rejected.

Two implementation declarations overlap when some valid substitutions can produce the same exact coherence key.

If the compiler cannot prove that two participating generic implementations are disjoint, they are rejected as overlapping.

Bray does not use specialization ranking between generic implementations.

An implementation is not selected because one implementation’s constraints look more specific than another’s.

If multiple participating generic implementations could produce the same exact coherence key, the program is invalid.

Trait generic parameters are inputs to the trait application.

Type-valued members are outputs of the selected trait implementation.

A generic trait should use generic parameters when the caller or constraint site chooses the type relationship.

A generic trait should use type-valued members when the implementation uniquely determines the related type.

For example, `ConvertTo<Target>` uses the target type as an input:

```bray
trait ConvertTo<Target>
{
    consume func convert() -> Target;
}
```

An iterator trait can use an element type as an implementation output:

```bray
trait Iterator
{
    type Element;

    mut func next() -> Element?;
}
```

### Trait implementation overload families

Multiple applications of the same generic trait for the same subject type are an implementation overload family.

Implementation overload families are explicit.

Different trait applications do not automatically form an implementation overload family.

When a subject type needs multiple applications of the same generic trait, each application is declared as a named implementation, and an overload declaration groups those implementation names under the shared subject and trait surface.

```bray
impl BufferBytesIterator = Buffer(Iterator<Bytes>)
{
    type Element = u8;

    mut func next() -> Element?
    {
        ...
    }
}

impl BufferLinesIterator = Buffer(Iterator<Lines>)
{
    type Element = Line;

    mut func next() -> Element?
    {
        ...
    }
}

overload Buffer(Iterator) =
{
    BufferBytesIterator,
    BufferLinesIterator,
}
```

The overload declaration has this form:

```bray
overload SubjectType(TraitName) =
{
    ImplementationName,
}
```

`SubjectType` is the shared subject type.

For a generic subject type, the overload header names the shared subject type declaration.

`TraitName` is the trait declaration whose applications are being grouped.

`ImplementationName` names a previously declared named trait implementation.

Each listed implementation must implement the same subject type and an application of the named trait declaration.

The overload declaration does not implement the trait.

It maps existing implementation identities to a shared subject and trait surface.

The overload name is the shared surface.

Each overload arm keeps its own implementation name.

Unnamed trait implementations cannot be listed in an implementation overload family.

An implementation overload family can list generic implementation declarations.

The family lists implementation declaration names.

It does not list instantiated implementation arms.

```bray
impl BufferValuesIterator = Buffer<T>(Iterator<Values>)
{
    type Element = T;

    mut func next() -> Element?
    {
        ...
    }
}

impl BufferIndexesIterator = Buffer<T>(Iterator<Indexes>)
{
    type Element = usize;

    mut func next() -> Element?
    {
        ...
    }
}

overload Buffer(Iterator) =
{
    BufferValuesIterator,
    BufferIndexesIterator,
}
```

For generic implementation arms, overlap checking is performed on the exact coherence keys produced by valid substitutions of each arm.

Every possible exact coherence key produced by one arm must be disjoint from every possible exact coherence key produced by every other arm in the same implementation overload family.

This is rejected:

```bray
impl BufferCloneItemsIterator = Buffer<T>(Iterator<Items>)
    with(T: Cloneable)
{
    ...
}

impl BufferCopyItemsIterator = Buffer<T>(Iterator<Items>)
    with(T: Copyable)
{
    ...
}

overload Buffer(Iterator) =
{
    BufferCloneItemsIterator,
    BufferCopyItemsIterator,
}
```

A type can satisfy both `Cloneable` and `Copyable`, so both arms can produce the same exact coherence key:

```text
(Buffer<T>, Iterator<Items>)
```

The overload family is invalid.

Bray does not use specialization ranking between implementation overload arms.

An implementation arm is not selected because its constraints look more specific than another arm’s constraints.

Generic trait arguments are part of the exact trait application.

Therefore, these are different implementation keys and can coexist when grouped:

```bray
impl BufferBytesIterator = Buffer(Iterator<Bytes>)
impl BufferLinesIterator = Buffer(Iterator<Lines>)
```

The same exact implementation key cannot appear more than once:

```bray
impl BufferBytesIterator = Buffer(Iterator<Bytes>)
impl BufferOtherBytesIterator = Buffer(Iterator<Bytes>) // invalid
```

If more than one participating implementation for the same subject type and generic trait declaration exists in a coherence domain, those implementations must be named and must be grouped by an implementation overload declaration.

A concrete non-overloaded trait implementation can use the unnamed `impl Type(TraitApplication)` form.

A generic trait implementation is a named implementation declaration with inferred generic parameters.

Method resolution through an implementation overload family follows overload resolution principles.

Receiver mode, receiver compatibility, member name, and explicitly supplied method arguments can select an overload arm.

Result type, expected type, and type-valued member outputs do not select an overload arm.

If no arm matches, the call is rejected.

If more than one arm matches, the call is rejected as ambiguous.

An exact trait application can be selected explicitly with a trait-qualified receiver expression:

```bray
buffer(Iterator<Bytes>).next()
```

This selects the `Iterator<Bytes>` implementation for the receiver before method lookup.

Trait-qualified receiver expression rules belong to the Expression Model.

### Trait use in constraints

Traits participate in generic constraints.

Generic constraints are written with a `with(...)` clause.

A `with(...)` clause is a static predicate-expression context.

Its entries are comma-separated static predicate expressions.

Static predicate expression rules belong to the Contract and Trust Model.

A static predicate expression can require that a type satisfy a trait application:

```bray
func max<T>(left: T, right: T) -> T
    with(T: Comparable<T>)
{
    ...
}
```

The trait application in a trait satisfaction constraint must be exact.

```bray
with(T: Comparable<T>)
```

If the trait declaration is generic, the constraint must supply the generic arguments required by that trait application.

If the trait declaration is not generic, the trait name alone is the exact trait application.

```bray
with(I: Iterator)
```

Static predicate expressions can also state type equality.

```bray
func first_token<I>(iter: I) -> Token?
    with(
        I: Iterator,
        I(Iterator).Element == Token,
    )
{
    return iter.next();
}
```

Type equality can relate type-valued members from different constrained types:

```bray
func zip_same<A, B>(left: A, right: B)
    with(
        A: Iterator,
        B: Iterator,
        A(Iterator).Element == B(Iterator).Element,
    )
{
    ...
}
```

Constraint facts are unordered.

The type-valued member equality can appear before or after the trait satisfaction constraint that makes the qualified reference valid.

The same constraint set must establish the exact trait application for the type-valued member reference to be accepted.

This is rejected:

```bray
func first_token<I>(iter: I) -> Token?
    with(
        I(Iterator).Element == Token,
    )
{
    ...
}
```

The equality mentions `I(Iterator).Element`, but the constraint set does not establish `I: Iterator`.

Type equality does not introduce a type alias.

Type equality does not select a trait implementation.

Type equality does not choose an arm from an implementation overload family.

Result type and expected type do not infer missing trait satisfaction constraints.

A generic body can use only operations, type-valued members, constants, effects, capabilities, and facts established by its static constraints and by surrounding declaration context.

### Trait method resolution

A method call can resolve to an inherent method or a trait method.

```bray
value.method(argument)
```

Method resolution uses:

- receiver type,
- receiver capability,
- inherent implementations,
- participating trait implementations,
- visible declarations,
- constraints,
- overload rules.

The selected method must match the receiver mode and argument binding supplied by the call.

The selected method must satisfy type checking, ownership checking, borrowing checking, capability checking, effect checking, and contract checking.

A method call resolves to exactly one callable.

Ambiguous method calls are rejected.

When method resolution sees an implementation overload family, resolution uses the same overload principles as callable overloads.

The receiver mode, receiver compatibility, member name, and explicitly supplied method arguments may select one arm.

Result type, expected type, and type-valued member outputs do not select an arm.

A trait-qualified receiver expression can select an exact trait application before member lookup.

Detailed method-call expression rules belong to the Expression Model.

### Static function resolution

A static function call can resolve to an inherent static function or trait static function.

```bray
Point.origin()
```

The path before the static function name determines the type, trait application, module, or package context used for resolution.

A static function call has no receiver.

Static function arguments follow the callable's parameter call surface.

Detailed static function call expression rules belong to the Expression Model.

### Trait coherence

For a given coherence domain, the exact coherence key for a trait implementation is:

```text
(ImplementingType, TraitApplication)
```

Any package can declare a trait implementation for any reachable subject type and trait application.

An implementation participates in a coherence domain only when the implementation is declared in that domain or explicitly imported into it.

A package dependency makes implementation declarations reachable for explicit import.

A package dependency does not silently make dependency implementations participate in the importing coherence domain.

Transitive dependency implementations do not participate unless they are explicitly imported or re-exported through ordinary import rules.

For each exact coherence key, Bray requires at most one participating implementation in a coherence domain.

Path qualification can name an implementation declaration.

Path qualification does not bypass coherence checking or activate an implementation for implicit trait satisfaction.

Generic arguments are part of the trait application.

Therefore these implementations have different exact coherence keys:

```bray
impl BufferBytesIterator = Buffer(Iterator<Bytes>)
impl BufferLinesIterator = Buffer(Iterator<Lines>)
```

These implementations have the same exact coherence key and are rejected:

```bray
impl BufferBytesIterator = Buffer(Iterator<Bytes>)
impl BufferOtherBytesIterator = Buffer(Iterator<Bytes>)
```

An implementation overload family groups multiple exact coherence keys that share an implementing type and trait declaration.

It does not allow duplicate exact coherence keys.

For generic implementations, the set of exact coherence keys produced by all valid substitutions must be disjoint from every other participating implementation in the same coherence domain.

Overlapping generic implementations are rejected.

This keeps method resolution, generic checking, and public API compatibility deterministic.

### Trait views and dynamic dispatch

Traits are behavioral contracts.

Traits are not value types.

Using a trait name as a stored type is rejected.

```bray
struct Logger
{
    sinks: [Sink; 4]; // invalid
}
```

Open heterogeneous storage through a trait uses a trait view behind an explicit storage or access type form.

```bray
struct Logger
{
    sinks: [box[Heap] view Sink; 4];
}
```

Dynamic dispatch in Bray is dispatch through a trait view.

It uses the implementation witness carried by the view.

It does not perform structural method lookup at runtime.

It does not search for methods by name at runtime.

It does not expose the hidden concrete type.

Generic constraints and trait views are separate forms of polymorphism.

A generic constraint keeps the concrete type known to the generic instantiation.

```bray
func write_all<S>(pos sink: S, pos message: String)
    with(S: Sink)
{
    sink.write(message);
}
```

A trait view hides the concrete type and dispatches through the selected implementation witness.

```bray
func write_one(pos sink: &view Sink, pos message: String)
{
    sink.write(message);
}
```

The trait-view type form, view-surface rules, receiver restrictions, and ownership behavior are defined by the trait-view type form.

### Trait API compatibility

A public trait is part of the public API.

Adding a required member to a public trait is a public API change.

Removing a member from a public trait is a public API change.

Changing a member name is a public API change.

Changing a member receiver mode is a public API change.

Changing parameter names or `pos` permissions is a public API change because they change the callable call surface.

Changing parameter types is a public API change.

Changing a result type is a public API change.

Adding a required type-valued member to a public trait is a public API change.

Removing a type-valued member from a public trait is a public API change.

Changing a type-valued member name is a public API change.

Changing the selected type for a public or reachable implementation can be a public API change.

Changing member contract clauses can be a public API change when requirements, guarantees, effects, capabilities, trusted obligations, or caller-visible behavior change.

Changing a default member body can be a public API change when observable behavior changes for implementations that use the default.

Changing trait visibility is a public API change.

Changing implementation visibility or reachability can be a public API change when it affects method resolution or generic satisfaction.

Changing implementation overload family membership can be a public API change when it affects trait satisfaction, method resolution, type-valued member selection, or generic satisfaction.

### Trait TODOs

TODO: Define constants in traits.

TODO: Define predicates in traits.

TODO: Define lifecycle declarations in traits.

---

## Design principles

Types are semantic contracts.

Every value has a type.

Every access path has a type and a capability state.

Types govern initialization, ownership, borrowing, mutation authority, movement, copying, destruction, finalization, layout, contracts, and valid operations.

Named types have identity.

Structural type forms produce types according to their type-form rules.

Product types model named-field structure.

Union types model closed tagged alternatives.

Traits model explicit behavioral contracts.

A type satisfies a trait through an explicit implementation.

Type forms exist only when the language needs compiler-known semantics that ordinary named types cannot express.

Default layout is compiler-defined.

Stable layout and foreign layout require explicit layout contracts.

Public type surfaces are API surfaces.

Internal type surfaces require explicit acknowledgement outside their intended scope.

Type behavior is explicit.

Polymorphism is explicit.

No type gains behavior by accidental structural matching.
