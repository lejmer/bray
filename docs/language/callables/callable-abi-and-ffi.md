# Callable ABI and FFI

A callable ABI is the representation and call-entry contract used when a callable crosses an ABI boundary.

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

Callable ABI modes, ABI-qualified callable types, foreign ABI representation, and callable ABI contract rules are defined in [Callable ABI](../targets-layout-abi-and-raw-memory/callable-abi.md).

```bray
let callback: @abi(c) func(pos left: i32, pos right: i32) -> i32 = compare;
```

Named callable contracts can name ABI-qualified callable type forms.

```bray
callable CompareCallback =
    @abi(c) func(pos left: i32, pos right: i32) -> i32;
```

### Extern callable declarations

The `extern` modifier declares a callable whose body is supplied by another linked artifact. The artifact can itself contain
separately compiled Bray. `extern` does not imply C or another foreign implementation language.

```bray
@link(name = "c")
@symbol(name = "getpid")
@abi(c)
extern trusted func get_process_id() -> i32
    uses(foreign_call);
```

Extern declarations, `@link(...)`, `@symbol(...)`, exported ABI callables, and foreign callbacks are defined in [Extern declarations and FFI](../targets-layout-abi-and-raw-memory/extern-declarations-and-ffi.md).

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Function declarations](function-declarations.md)
- Next: [Const functions](const-functions.md)
