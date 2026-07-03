# Union Types

## Union type

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

## Variant declarations

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

Payload fields are named by default.

A payload field can use the `pos` modifier to permit positional construction and positional pattern matching.

```bray
union Result<T, E>
{
    Ok(pos value: T);
    Error(pos error: E);
}
```

Payload field declarations use `:`.

Payload field declarations inside variant parentheses are comma-separated.

Variant names must be unique within the union type.

Payload field names must be unique within the variant payload.

A `pos` payload field may only appear before any non-`pos` payload field in the same variant payload.

Variant-level visibility modifiers are excluded from union bodies.

A union’s variant surface has the visibility of the union type.

## Variant payload fields

A payload field is a named value stored by a payload variant.

```bray
Circle(center: Point, radius: r64);
```

Each payload field has a name, a type, a call-position permission, a mutability contract, an optional default expression, and an
initialization state.

The `pos` modifier changes the construction and pattern surface only.

The payload field still has a name, and payload field access uses that name.

A `pos` payload field can be supplied positionally or by name.

A `pos` payload field can be matched positionally or by name.

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

## Variant payload defaults

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

Construction-time payload default behavior is defined in [Union variant construction expressions](../expressions/union-variant-construction-expressions.md).

A variant payload default is checked in the union declaration context.

A variant payload default cannot reference sibling payload fields.

A variant payload default cannot reference `self`.

A variant payload default is evaluated when that payload field is omitted during construction.

Effects of an evaluated payload default become effects of the variant construction expression.

Finalization obligations created by an evaluated payload default become obligations of the constructed union value, local temporaries, or surrounding context according to ownership and lifecycle rules.

Trusted capabilities used by a payload default must be permitted by the declaration context and construction context according to trusted capability rules.

## Variant contracts

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

## Union construction

Union construction is handled by union variant construction expressions.

```bray
let shape = Shape.Circle(center = origin, radius = 10.0);

let shape: Shape = .Circle(center = origin, radius = 10.0);

let end = ParseResult<i32>.EndOfInput;

let end: ParseResult<i32> = .EndOfInput;
```

Union variant construction expressions initialize the selected variant payload and create fully initialized union values.

Union variant construction expression rules are defined in [Union variant construction expressions](../expressions/union-variant-construction-expressions.md).

## Active variant

Every fully initialized union value has one active variant.

The active variant is part of the union value’s runtime state.

Operations that branch on or refine a union value can make the active variant known within a control-flow region.

When the active variant is known, the selected payload exists in that region.

Payload field access requires active-variant refinement proving that the selected payload exists.

A no-payload active variant introduces no payload field access paths.

Active-variant facts participate in the fact context.

Mutation, movement, consumption, destruction, reinitialization, finalization, or capability loss can invalidate active-variant facts when they affect the union value or its storage.

## Union access paths

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

## Union replacement

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

## Union initialization state

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

## Partial moves

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

## Union movement

Moving a fully initialized union value moves the complete union.

Moving the complete union transfers ownership of the active tag and active payload to the new owner.

The old access path becomes moved-from until reinitialized.

Moving a union preserves the union type and active variant.

Moving a union transfers finalization obligations carried by the union or its active payload to the new owner.

## Union copying

A union value is copyable only when the union type has an accepted `@copy` contract and every possible active payload satisfies the
required copy contract for that concrete type.

Copying a union copies the active tag.

Copying a union copies the active payload according to the active payload field types’ copy contracts.

Inactive payloads have no initialized values to copy.

Copying a union produces a separate value with its own ownership story.

Copy behavior is explicit through the union type's `@copy` contract.

## Union destruction

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

## Union lifecycle declarations

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

Union constructors create fully initialized values of the union type.

Lifecycle behavior can depend on the active variant.

Union lifecycle declarations follow the [lifecycle declaration](lifecycle-declarations.md) ordering, signature, and selection rules.

Union lifecycle declarations are whole-union lifecycle declarations.

Variant-dependent lifecycle behavior is expressed inside whole-union lifecycle bodies with ordinary control flow, pattern matching,
active-variant refinement, and payload access.

A successfully constructed union carries:

- the lifecycle obligations declared by the union type,
- the lifecycle obligations of the active payload,
- any lifecycle obligations produced by payload defaults or constructor body expressions.

Payload defaults used during construction are evaluated according to union construction rules before the union becomes fully
initialized.

A finalizer can branch on `self` with ordinary `match`.

A successful variant pattern in a finalizer refines `self` to that active variant and makes initialized payload fields available
according to the pattern operation mode.

A finalizer can observe and mutate active payload fields when its declaration contract permits those operations.

A destructor can branch on `self` with ordinary `match`.

A successful variant pattern in a destructor refines `self` to that active variant and makes initialized payload fields available
according to the pattern operation mode.

A destructor can observe and mutate active payload fields when its declaration contract permits those operations.

If a destructor consumes or destroys an active payload field, that field becomes uninitialized and is not destroyed again after the
destructor returns.

Any initialized active payload fields remaining after the destructor returns are destroyed according to active payload destruction
rules.

The scoped capability can borrow from the union, carry access authority for the union or its active payload, or carry an
independent resource token, according to the scoped capability type.

An active scoped capability can restrict observation, mutation, borrowing, movement, active-variant replacement, finalization,
destruction, and partial moves of the union for the lifetime of the `with` body.

Whole-union replacement resolves the old active variant payload according to union finalization, destruction, and active payload
destruction rules before the new active variant tag and payload become initialized at that access path.

Variant declarations remain the source of variant names and payload structure.

## Recursive unions

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

## Union layout

A union has a semantic active variant tag.

The compiler chooses the default physical tag representation and payload layout.

The default layout can use representation optimizations when Bray semantics are preserved.

Default layout is compiler-defined.

Stable ABI layout, C-compatible layout, explicit tag representation, packed layout, and explicit alignment belong to explicit
layout contracts.

Union layout is declared with `@layout(...)` immediately before the `union` declaration.

```bray
@layout(stable, tag = u8)
union Message
{
    @tag(1)
    Ready;

    @tag(2)
    Data(bytes: [u8; 16]);
}
```

Union types accept these layout modes:

- `stable`,
- `c`.

`stable` union layout defines deterministic tag representation, payload layout, size, and alignment for the target layout
profile.

If `tag` is omitted from a `stable` union layout, the tag type is the smallest fixed-width unsigned integer scalar type that can
represent every variant tag value.

If no fixed-width unsigned integer scalar type can represent every variant tag value, the union must declare `tag` explicitly.

`c` union layout uses a C-compatible tagged aggregate representation for the target C ABI.

An explicitly laid out `c` union must declare `tag`.

```bray
@layout(c, tag = u32)
union CStatus
{
    @tag(0)
    Ok;

    @tag(1)
    Error(code: u32);
}
```

The `@tag(value)` directive declares a physical tag value for a union variant.

Tag values are compile-time integer constants.

Tag values must be representable by the union's physical tag type.

Tag values must be unique within the union.

If any variant in an explicitly laid out union uses `@tag`, every variant in that union must use `@tag`.

If no variant in an explicitly laid out union uses `@tag`, variant tag values are assigned by declaration order starting at `0`.

## Union patterns

Union values can be refined and decomposed by variant patterns.

```bray
Circle(center = c, radius = r)
Empty
```

Variant patterns refine the subject to the matched active variant in the matched region.

Payload variant patterns introduce bindings for selected payload fields according to the pattern operation mode.

No-payload variant patterns introduce no payload bindings.

Union variant pattern rules are defined in [Union variant patterns](../patterns/union-variant-patterns.md).

Match expressions over closed unions perform coverage checking against the union’s closed variant set.

Union patterns participate in ownership, borrowing, copying, partial moves, initialization, destruction, finalization, capability checking, and fact-context refinement according to the pattern operation mode.

## Compiler-known result unions

`Result<T, E>` is the compiler-known union type for recoverable domain failure and caught synchronous panic values.

The Compiler-Known and Standard Library Model defines availability and import behavior for compiler-known declarations and
standard-library declarations.

Its semantic declaration is:

```bray
union Result<T, E>
{
    Ok(pos value: T);
    Error(pos error: E);
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
    Completed(pos value: T);
    Panicked(pos report: PanicReport);
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

`ConversionError` is the compiler-known error type used by built-in fallible conversions.

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

`ConversionError.NonFinite` means the source value is not finite and the selected fallible conversion contract requires a finite value.

`ConversionError.NonRepresentable` means the source value is within the target type's range and domain but cannot be represented exactly by the target type without rounding, truncation, saturation, wrapping, or other information loss.

`ConversionError` reports the failure category only.

It does not carry the source value, source type, target type, or source expression location.

Diagnostics can report those details from the conversion operation and type-checking context.

User-defined fallible conversions do not use `ConversionError` unless their selected `CheckedConvertTo<Target>.Error` type is `ConversionError`.

## Compiler-known task handles

`Task<T>` is the compiler-known linear task handle type for a spawned asynchronous task whose ordinary result type is `T`.

`Task<T>` is an owned value.

`Task<T>` is not copyable.

Moving a `Task<T>` transfers the task obligation.

Joining or cancelling a `Task<T>` consumes the handle and resolves the task obligation.

The Async Model defines task handle creation, joining, cancellation, transfers, escape rules, borrowing rules, and obligation
checking.

## Compiler-known thread handles

`Thread<T>` is the compiler-known linear thread handle type for a spawned synchronous thread whose ordinary result type is `T`.

`Thread<T>` is an owned value.

`Thread<T>` is not copyable.

Moving a `Thread<T>` transfers the thread obligation.

Joining or cancelling a `Thread<T>` consumes the handle and resolves the thread obligation.

The Async Model defines thread handle creation, joining, cancellation, transfers, escape rules, borrowing rules, capture
restrictions, and obligation checking.

## Union API compatibility

For a public union type, the variant set is part of the public API.

Adding, removing, or renaming a public union variant is a public API change.

Changing a payload field name is a public API change.

Changing a payload field type is a public API change.

Changing payload field mutability is a public API change.

Changing payload defaults can be a public API change when construction behavior visible to users changes.

Changing variant contracts can be a public API change when construction requirements or established facts visible to users change.

Changing union lifecycle declarations follows the public API compatibility rule defined in [Lifecycle declarations](lifecycle-declarations.md#api-compatibility).

Default physical layout is not public ABI unless the type declares an explicit layout contract.

## Navigation

- [Language index](../index.md)
- [Types index](../types.md)
- Previous: [Product Types](product-types.md)
- Next: [Type Forms](type-forms.md)
