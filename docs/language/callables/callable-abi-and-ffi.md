# Callable ABI and FFI

[Callable ABI](../targets-layout-abi-and-raw-memory/callable-abi.md) defines call-entry contracts, ABI modes,
ABI-qualified callable types, accepted foreign representations, and variadic contracts.

For example, a callable can declare its ABI with `@abi(...)`.

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

The following examples illustrate ABI-qualified callable values and named callable contracts.

```bray
let callback: @abi(c) func(pos left: i32, pos right: i32) -> i32 = compare;
```

```bray
callable CompareCallback =
    @abi(c) func(pos left: i32, pos right: i32) -> i32;
```

## Extern callable declarations

[Extern declarations](../targets-layout-abi-and-raw-memory/extern-declarations-and-ffi.md) define externally supplied
callable bodies. The following example illustrates the link, symbol, ABI, and trust declarations.

```bray
@link(name = "c")
@symbol(name = "getpid")
@abi(c)
extern trusted func get_process_id() -> i32
    uses(foreign_call);
```

Extern callables, `@link(...)`, `@symbol(...)`, exported ABI callables, and foreign callbacks are defined in
[Extern declarations and FFI](../targets-layout-abi-and-raw-memory/extern-declarations-and-ffi.md#link-and-symbol-directives). Extern static
storage and native data symbols are defined in
[Foreign data and symbols](../targets-layout-abi-and-raw-memory/foreign-data-and-symbols.md).

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Function declarations](function-declarations.md)
- Next: [Const functions](const-functions.md)
