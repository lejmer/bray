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
- optional types,
- borrow types,
- owned-indirection types,
- callable types.

Scalar types include integers, real floating-point types, complex floating-point types, machine-sized integer types, `bool`, `char`, `unit`, and `never`.

Product types are named types with fields.

Union types are closed tagged sum types with variants.

Tuple types are fixed-size ordered product types.

Fixed-size array types are fixed-size ordered homogeneous product types.

Optional types are produced by the postfix optional type form `T?`.

Borrow types are produced by `&T` and `&mut T`.

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

TODO: Define generic syntax, constraints, and generic checking.

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
    delay: Duration = Duration.seconds(value = 1);
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

TODO: Define the grammar order for field visibility and mutability modifiers.

Recommended canonical order:

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
    delay: Duration = Duration.seconds(value = 1);
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

TODO: Define how custom lifecycle behavior restricts or extends product partial-move rules.

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

Destroying a fully initialized product value destroys its initialized fields.

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

A product type can define lifecycle declarations inside its type body.

```bray
struct File
{
    handle: OsHandle;

    construct File(path: Path, mode: FileMode = FileMode.read) -> File
    {
        ...
    }

    construct temp(directory: Path, prefix: String = "tmp") -> File
    {
        ...
    }

    async finalize File() -> Result<unit, FileError>
    {
        ...
    }

    destruct File()
    {
        ...
    }
}
```

Constructors create fully initialized values of the product type.

A constructor named after the type is the primary constructor form.

A constructor with another name becomes a named constructor under the type:

```bray
let my_file = File.temp(directory = some_path);
```

Finalizers complete required lifecycle obligations before ownership ends.

Destructors perform synchronous cleanup when ownership ends.

Scope enter and exit declarations define scoped capability behavior.

Lifecycle declarations participate in ownership, borrowing, mutation authority, finalization obligations, effects, and trusted capability checking.

TODO: Define detailed lifecycle rules for product types.

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

Field shorthand binds a field to a binding with the same name.

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
    Retry(count: i32 = 3, delay: Duration = Duration.seconds(value = 1));
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

TODO: Define how custom lifecycle behavior restricts or extends union partial-move rules.

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

Destroying a fully initialized union value destroys the active payload.

Inactive variant payloads have no initialized values and therefore no destruction work.

A no-payload active variant has no payload fields to destroy.

A partially initialized union destroys only initialized fields of the active payload.

A moved-from payload field is not destroyed by the old union owner.

A union type can define a destructor with a `destruct` lifecycle declaration.

A union destructor is synchronous and returns `unit`.

Fallible or asynchronous cleanup belongs to finalization.

A union value with finalization obligations must satisfy those obligations before ownership ends, unless the value is transferred to another owner that assumes them or converted into an explicit fallback ownership form.

### Union lifecycle declarations

A union type can define lifecycle declarations inside its type body.

```bray
union ResourceState
{
    Open(handle: OsHandle);
    Closed;

    destruct ResourceState()
    {
        ...
    }
}
```

Constructors create fully initialized values of the union type.

Finalizers complete required lifecycle obligations before ownership ends.

Destructors perform synchronous cleanup when ownership ends.

Scope enter and exit declarations define scoped capability behavior.

Lifecycle declarations participate in ownership, borrowing, mutation authority, finalization obligations, effects, and trusted capability checking.

Lifecycle behavior can depend on the active variant.

TODO: Define detailed lifecycle rules for union types.

TODO: Define variant-specific lifecycle declarations.

### Recursive unions

A union can be recursive through explicit indirection.

A recursive cycle in a union type must pass through an indirection boundary.

An indirection boundary is a type form or type whose outer representation has statically known finite size independent of the recursively referenced type.

The primary owned-indirection boundary is `box`.

```bray
union List<T>
{
    Node(value: T, next: box List<T>);
    Empty;
}
```

A recursive union that contains itself by value in a cycle without indirection has no finite size and is rejected.

```bray
union BadList<T>
{
    Node(value: T, next: BadList<T>);
    Empty;
}
```

TODO: Define the type forms and type categories that count as recursive indirection boundaries.

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
.Circle(center = c, radius = r)
.Empty
```

Variant patterns refine the subject to the matched active variant in the matched region.

Payload variant patterns introduce bindings for selected payload fields according to the pattern operation mode.

No-payload variant patterns introduce no payload bindings.

Union patterns are checked by the Pattern Model.

Match expressions over closed unions perform coverage checking against the union’s closed variant set.

Union patterns participate in ownership, borrowing, copying, partial moves, initialization, destruction, finalization, capability checking, and fact-context refinement according to the pattern operation mode.

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
box T
box[Heap] T
T?
[T; N]
(T1, T2)
func(left: T1, right: T2) -> R
```

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

A **prefix type form** appears before its subject type.

```bray
&T
&mut T
box T
box[Heap] T
```

`&T` is the shared-borrow type form.

`&mut T` is the mutable-borrow type form.

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

`T?` is the optional type form.

It produces a type whose values represent either a present `T` value or the absence state.

TODO: Define optional absence spelling and optional handling syntax.

Postfix type forms compose with other type forms according to the type grammar.

```bray
Point?
box[Heap] Point?
```

Each postfix type form contributes its own value-state, ownership, initialization, destruction, finalization, and pattern behavior.

### Structural type forms

A **structural type form** uses a larger syntactic structure to produce a type.

```bray
[T; N]
(T1, T2)
func(left: T1, right: T2) -> R
```

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
[box[Heap] Node; 4]
func(buffer: &Buffer, index: usize) -> u8
```

The meaning of a composed type is determined by applying each type form according to the type grammar and the semantic contract of that form.

Composition order is determined by the type grammar.

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

### Optional type form

The optional type form is:

```bray
T?
```

An optional value is either a present contained value of type `T` or the absence state.

The absence state is a valid initialized state.

An optional value is fully initialized when it is initialized to either presence or absence.

When present, the contained `T` value follows the ownership, borrowing, movement, copying, destruction, finalization, and capability rules of `T`.

When absent, there is no contained `T` value to access, move, copy, borrow, destroy, or finalize.

Optional-to-optional conversion follows the recursive explicit convertibility rule when the contained source type is explicitly convertible to the contained target type.

The absence state remains absence during optional-to-optional conversion.

A present value is converted recursively.

TODO: Define optional construction, absence literal syntax, unwrapping syntax, propagation syntax, and optional patterns.

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

The storage policy type must satisfy the storage behavior required for storing `T`.

The storage policy is an ordinary type with compiler-recognized storage behavior.

TODO: Define the `Storage` trait and storage model.

A `box[S] T` value owns separately stored `T`.

Moving a `box[S] T` moves ownership of the indirection value.

Destroying a `box[S] T` destroys the contained `T` and releases storage according to the storage policy.

Borrowing a `box[S] T` can project a borrow of the contained `T` when the storage policy and access path permit it.

Mutable borrowing a `box[S] T` can project mutable access to the contained `T` when the box access path, storage policy, and contained type permit it.

The outer representation of `box[S] T` has statically known finite size independent of `T`.

`box[S] T` can serve as an indirection boundary for recursive types.

Box construction is handled by box construction expressions.

```bray
let node: box List<i32> = box(.Empty);
```

Detailed box construction rules belong to the Expression Model.

### Fixed-size array type form

The fixed-size array type form is:

```bray
[T; N]
```

`T` is the element type.

`N` is the array length.

`N` is part of the type.

A fixed-size array contains exactly `N` elements of type `T`.

Each element has its own initialization state while the array is being initialized or after a partial move.

An array value is fully initialized when every element is initialized.

TODO: Define array indexing rules.

Array construction is handled by array expressions and array generator expressions.

TODO: Define array movement, copying, borrowing, partial moves, destruction, and finalization rules.

Detailed array expression rules belong to the Expression Model.

TODO: Define array indexing expression rules.

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

Each tuple element has its own type and initialization state.

A tuple value is fully initialized when every element is initialized.

Tuple construction is handled by tuple expressions.

TODO: Define tuple movement, copying, borrowing, partial moves, destruction, and finalization rules.

Detailed tuple expression rules belong to the Expression Model.

TODO: Define tuple element projection syntax.

### Callable type form

The callable type form is:

```bray
func(parameter: Type, ...) -> Result
```

A callable type describes a callable value’s parameter names, parameter types, result type, execution mode, ownership behavior, borrowing behavior, mutation requirements, lifetime requirements, capability requirements, effects, trusted caller obligations, and finalization behavior.

Callable parameter names are part of the callable contract because function call arguments are named.

```bray
func(left: i32, right: i32) -> i32
```

A callable returning `unit` can omit the result type in declarations, but callable type forms spell result behavior according to the callable type grammar.

Callable types preserve caller-visible obligations.

A callable with trusted caller obligations requires a callable type that preserves those obligations.

Callable values and callable declarations are checked by the Function and Callable Model.

TODO: Define callable type syntax with contract clauses.

TODO: Define async callable type syntax.

TODO: Define closure and anonymous callable type interactions.

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

A recursive type cycle must pass through an indirection boundary when the cycle would otherwise make the type infinitely sized.

A type form can be an indirection boundary when its outer representation has statically known finite size independent of the recursively referenced subject type.

`box[S] T` is an indirection boundary.

TODO: Define other indirection boundaries.

TODO: Define recursive sizing rules.

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

`?` is a type form because optionality is a core value-state shape used throughout the language.

`func(...) -> ...` is a type form because callable values carry parameter, result, ownership, effect, execution, and contract semantics.

---

## Traits

A **trait** is a named behavioral contract.

A trait defines behavior that a type can satisfy through an explicit implementation.

Traits are declared with `trait`.

```bray
trait Equatable
{
    func equals(other: &Self) -> bool;
}
```

A trait declaration introduces a named compile-time entity.

A trait is used by implementations, constraints, method resolution, callable checking, and public API compatibility.

A trait can be generic.

```bray
trait Comparable<Other>
{
    func compare(other: &Other) -> Ordering;
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
trait Equatable
{
    func equals(other: &Self) -> bool;
}

internal trait Storage<T>
{
    func borrow() -> &T;
}
```

A public trait exposes its full contract surface as public API.

An internal trait is available within its intended scope.

Use of an internal trait outside its intended scope requires explicit internal-use acknowledgement.

Trait members inherit the visibility of the trait.

The grammar excludes `public` and `internal` modifiers on individual trait members.

```bray
trait Equatable
{
    func equals(other: &Self) -> bool;
}
```

The full trait body is the contract surface of the trait.

### Trait member declarations

A trait body contains member declarations that make up the trait’s behavioral contract.

The currently defined trait member form is the callable member declaration.

```bray
trait Equatable
{
    func equals(other: &Self) -> bool;
}
```

A trait callable member can be required or defaulted.

A required trait member has no body and ends with a semicolon.

```bray
trait Equatable
{
    func equals(other: &Self) -> bool;
}
```

A defaulted trait member has a body.

```bray
trait Equatable
{
    func equals(other: &Self) -> bool;

    func not_equals(other: &Self) -> bool
    {
        ...
    }
}
```

A defaulted member body provides default behavior for implementations that do not supply that member.

A defaulted member body is checked in trait context.

A defaulted member body can use the trait’s declared surface, `self` when the member is an instance method, `Self`, trait parameters, available constraints, and declarations visible from the trait declaration context.

A defaulted member body must satisfy the member’s declared result type, ownership behavior, borrowing behavior, capability contract, effect contract, and contract clauses.

TODO: Define additional trait member kinds such as constants, predicates, lifecycle requirements, and type-valued members.

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

`self` is not declared as an ordinary parameter.

An instance method’s ordinary parameters are written inside the parameter list.

```bray
trait Comparable<Other>
{
    func compare(other: &Other) -> Ordering;
}
```

The receiver is supplied by method-call syntax.

```bray
point.compare(other = other)
```

Function-call arguments are always named. Method arguments other than the receiver are also named.

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
    static func parse(text: String) -> T;
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
    func compare(other: &Other) -> Ordering
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

TODO: Define trusted implementation capability clauses on required trait members.

TODO: Define effect annotation syntax beyond currently defined contract clauses.

## Implementations

An **implementation** declares behavior for a type.

Implementations are declared with `impl`.

An inherent implementation is written as `impl Type`.

```bray
impl Point
{
    func distance_to(other: &Self) -> r64
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

A trait implementation is written as `impl Type(TraitApplication)`.

```bray
impl Point(Equatable)
{
    func equals(other: &Self) -> bool
    {
        ...
    }
}
```

For a generic trait application:

```bray
impl Point(Comparable<Point>)
{
    func compare(other: &Point) -> Ordering
    {
        ...
    }
}
```

For a generic implementing type:

```bray
impl Buffer<T>(Storage<T>)
{
    func borrow() -> &T
    {
        ...
    }
}
```

A trait implementation makes the implementing type satisfy the specified trait application.

Trait satisfaction is explicit.

A type satisfies a trait application through an `impl Type(TraitApplication)` declaration.

Matching member names and signatures alone gives no trait satisfaction.

### Implementation members

An implementation body contains member definitions.

In an inherent implementation, member definitions become behavior associated with the implementing type.

In a trait implementation, member definitions fulfill members of the implemented trait application.

```bray
impl Point(Equatable)
{
    func equals(other: &Self) -> bool
    {
        ...
    }
}
```

A trait implementation must provide every required trait member that has no default body.

A trait implementation can provide a member that has default behavior in the trait.

When an implementation provides a member with default behavior, the implementation member is used for that implementation.

When an implementation omits a member with default behavior, the trait’s default behavior is used for that implementation.

An implementation member must match the fulfilled trait member’s name, receiver mode, parameter names, parameter types, result type, execution mode, contract obligations, and caller-visible effects.

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
    func distance_to(other: &Self) -> r64
    {
        ...
    }
}
```

The receiver is supplied by method-call syntax.

```bray
point.distance_to(other = other)
```

`self` is available only inside instance method bodies.

Static function bodies use `Self` for the implementing type and receive ordinary parameters through their parameter list.

### Generic traits

A trait can have generic parameters.

```bray
trait Comparable<Other>
{
    func compare(other: &Other) -> Ordering;
}
```

A trait implementation supplies a concrete trait application.

```bray
impl Point(Comparable<Point>)
{
    func compare(other: &Point) -> Ordering
    {
        ...
    }
}
```

A generic implementation can satisfy a family of trait applications.

```bray
impl Box<T>(Comparable<Box<T>>)
{
    func compare(other: &Box<T>) -> Ordering
    {
        ...
    }
}
```

TODO: Define generic parameter syntax, generic constraints, and generic implementation checking for implementations.

### Trait use in constraints

Traits participate in generic constraints.

A constraint can require that a type satisfy a trait application.

```bray
func max<T>(left: T, right: T) -> T
    with(T: Comparable<T>)
{
    ...
}
```

TODO: Define constraint syntax and checking for trait requirements.

### Trait method resolution

A method call can resolve to an inherent method or a trait method.

```bray
value.method(argument = argument)
```

Method resolution uses:

- receiver type,
- receiver capability,
- inherent implementations,
- visible trait implementations,
- visible declarations,
- constraints,
- overload rules.

The selected method must match the receiver mode and argument names supplied by the call.

The selected method must satisfy type checking, ownership checking, borrowing checking, capability checking, effect checking, and contract checking.

A method call resolves to exactly one callable.

Ambiguous method calls are rejected.

Detailed method-call expression rules belong to the Expression Model.

### Static function resolution

A static function call can resolve to an inherent static function or trait static function.

```bray
Point.origin()
```

The path before the static function name determines the type, trait application, module, or package context used for resolution.

A static function call has no receiver.

Static function arguments are named.

Detailed static function call expression rules belong to the Expression Model.

### Trait coherence

For a given implementing type and trait application, Bray requires a single visible implementation within the relevant coherence domain.

This keeps method resolution, generic checking, and public API compatibility deterministic.

TODO: Define coherence domains for trait implementations.

### Trait objects and dynamic dispatch

TODO: Define trait objects, dynamic dispatch type forms, vtable-like representation, object safety, receiver restrictions, and ownership behavior.

### Trait API compatibility

A public trait is part of the public API.

Adding a required member to a public trait is a public API change.

Removing a member from a public trait is a public API change.

Changing a member name is a public API change.

Changing a member receiver mode is a public API change.

Changing parameter names is a public API change because callable arguments are named.

Changing parameter types is a public API change.

Changing a result type is a public API change.

Changing member contract clauses can be a public API change when requirements, guarantees, effects, capabilities, trusted obligations, or caller-visible behavior change.

Changing a default member body can be a public API change when observable behavior changes for implementations that use the default.

Changing trait visibility is a public API change.

Changing implementation visibility or reachability can be a public API change when it affects method resolution or generic satisfaction.

### Trait TODOs

TODO: Define constants in traits.

TODO: Define predicates in traits.

TODO: Define lifecycle declarations in traits.

TODO: Define type-valued trait members.

TODO: Define generic constraints.

TODO: Define coherence domains.

TODO: Define dynamic dispatch.

TODO: Define trait object and trait type-form syntax.

TODO: Define operator traits.

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
