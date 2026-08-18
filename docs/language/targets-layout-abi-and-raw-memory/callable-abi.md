# Callable ABI

A callable ABI is the representation and call-entry contract used when a callable crosses an ABI boundary.

Ordinary Bray calls use Bray's default callable ABI.

The default callable ABI is compiler-defined and is not an external ABI contract.

The `@abi(...)` directive selects an explicit callable ABI for a callable declaration or callable type form.

```bray
@abi(c)
func compare(pos left: i32, pos right: i32) -> i32
{
    if left < right
    {
        return -1;
    }

    if left > right
    {
        return 1;
    }

    return 0;
}
```

Callable ABI directive syntax:

```bray
@abi(mode, option = value, ...)
```

The first argument is the ABI mode.

Remaining arguments are named ABI options.

The ABI modes are:

- `c`,
- `system`.

`c` uses the selected target's C callable ABI.

`system` uses the selected target's system callable ABI for platform APIs.

The selected target profile defines the exact calling convention, register and stack rules, scalar widening rules,
symbol format, and platform availability for each ABI mode.

Only one `@abi(...)` directive can apply to a callable declaration or callable type form.

`@abi(...)` is part of the callable contract.

A callable value satisfies an ABI-qualified callable type only when the callable exposes the same ABI contract.

```bray
let callback: @abi(c) func(pos left: i32, pos right: i32) -> i32 = compare;
```

A callable type without `@abi(...)` requires Bray's default callable ABI.

An ABI-qualified callable type is not interchangeable with an otherwise identical callable type that uses Bray's default
callable ABI.

Named callable contracts can name ABI-qualified callable type forms.

```bray
callable CompareCallback =
    @abi(c) func(pos left: i32, pos right: i32) -> i32;
```

For `c` and `system` ABI callables, by-value parameters and results must have an ABI representation accepted by the
selected ABI.

Accepted foreign ABI representation categories are:

- scalar types accepted by the selected target ABI,
- `unit` as a callable result with no returned value,
- raw pointer values,
- ABI-qualified callable values with the same ABI,
- products and unions whose explicit layout contract is accepted by the selected ABI.

For `c`, accepted aggregate layout contracts are `@layout(c)` and compatible `@layout(transparent)`.

Borrow types, slices, default-layout products, default-layout unions, trait-view types, owned-indirection types, async
computations, task handles, and callable values without the selected foreign ABI contract need an explicit ABI wrapper
or lowering declaration before they can cross a foreign ABI boundary.

`@abi(...)` does not change ownership, borrowing, lifetime, panic, contract, trusted capability, generic, overload, or
evaluation rules.

`@abi(...)` does not make a type's data layout public ABI.

Data layout is controlled by `@layout(...)` and the standard layout helpers.

## Variadic callable contracts

An ellipsis after one or more fixed parameters declares a variadic foreign callable contract.

```bray
@abi(c)
extern trusted func printf(
    pos format: RawPointer<std.ffi.c.char>,
    ...
) -> std.ffi.c.int
    uses(foreign_call);
```

Variadic syntax is valid only for an `extern trusted` function or an ABI-qualified callable type using a target ABI that
defines variadic calls. A Bray function body, lambda, async callable, const callable, default parameter, named variadic
argument, or exported Bray variadic implementation is invalid.

The fixed parameters are checked normally. Every trailing call argument is positional, is evaluated in source order, and
must have a foreign ABI representation accepted in the selected target's variadic position.

The compiler applies the exact default argument promotions required by the selected ABI. For the C ABI this includes
integer promotions and promotion of `r32` to `r64`. No Bray borrow, owned value, trait view, protected representation,
task, async computation, or default-layout aggregate is implicitly converted into a variadic representation.

Target ABI classification determines register, stack, alignment, and aggregate passing. Unsupported promoted types and
target combinations are rejected before code generation.

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [Layout helpers](layout-helpers.md)
- Next: [Extern declarations and FFI](extern-declarations-and-ffi.md)
