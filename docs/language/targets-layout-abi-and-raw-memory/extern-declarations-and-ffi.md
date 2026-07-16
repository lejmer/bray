# Extern declarations and FFI

The `extern` modifier declares a callable whose implementation body is supplied by another linked artifact rather than by this
declaration.

An extern callable declaration has no Bray body and ends with `;`.

The providing artifact can contain separately compiled Bray code, compiler-generated runtime code, a platform or system library, or
code written in another language. `extern` therefore does not by itself mean C, FFI, or foreign code. The selected callable ABI and
link dependency determine whether the call crosses a foreign boundary.

```bray
@link(name = "c")
@symbol(name = "getpid")
@abi(c)
extern trusted func get_process_id() -> i32
    uses(foreign_call);
```

`extern` declarations do not create unqualified names outside their declaration context.

Name resolution, visibility, module membership, using declarations, overload declarations, callable type checking, contract checking, and trusted obligation checking apply normally.

An extern callable that crosses a foreign ABI must declare:

- an explicit `@abi(...)` directive,
- an external symbol through `@symbol(...)`,
- a link dependency through `@link(...)` on the declaration or containing module,
- `trusted`,
- `uses(foreign_call)`.

The `uses(foreign_call)` capability is consumed by the external call boundary.

An extern trusted declaration is permitted only in a trusted module.

The extern declaration's signature and contract are the Bray-visible contract for the linked symbol.

If the foreign symbol requires pointer validity, initialization, alignment, lifetime, ownership, thread-affinity, callback, reentrancy, or resource-state facts, those facts must appear in the declaration's parameter types, result types, or contract clauses.

Imported foreign failure modes are represented as ordinary ABI values.

Bray `Result<T, E>` can cross a foreign ABI boundary only when its representation is accepted by that boundary through an explicit layout contract.

## Link and symbol directives

`@link(...)` selects a declared external artifact or system library dependency.

```bray
@link(name = "z")
trusted module ffi.zlib
{
    ...
}
```

`@link(...)` can attach to a module declaration or to an extern callable declaration.

A module-level `@link(...)` applies to extern declarations in that module that do not declare their own `@link(...)`.

`@link(...)` does not discover, fetch, build, or version an external library.

It selects a dependency graph node supplied by package metadata, build configuration, or the selected target profile.

If no matching dependency graph node exists for the selected target, the declaration is rejected.

The required `@link(...)` arguments are:

- `name = "..."`.

Optional `@link(...)` arguments are target-profile defined.

The standard option names are:

- `kind = dynamic`,
- `kind = static`,
- `kind = system`,
- `kind = framework`.

A target profile accepts only the link kinds it supports.

`@symbol(...)` binds a declaration to an external symbol name.

```bray
@symbol(name = "zlibVersion")
```

`@symbol(...)` can attach to an extern callable declaration or to an ABI-qualified Bray callable declaration exported as a native symbol.

For extern callable declarations, `@symbol(...)` names the symbol that the linker or loader must resolve.

For exported Bray callable declarations, `@symbol(...)` names the native symbol made visible to foreign code.

The `@symbol(...)` name is an exact external symbol identity after the selected target profile's symbol encoding rules are applied.

## Exported ABI callables

A Bray callable with a body can be exported through an explicit ABI and symbol.

```bray
@symbol(name = "bray_add_i32")
@abi(c)
func add_i32(pos left: i32, pos right: i32) -> i32
{
    return left + right;
}
```

An exported ABI callable is type checked as an ordinary Bray callable.

It is not `extern` because its implementation is Bray source.

It is `trusted` only when its body uses trusted implementation capabilities or its declaration exposes trusted caller obligations.

An uncaught Bray panic must not unwind through a foreign ABI frame.

If an uncaught panic reaches an exported non-Bray ABI boundary, the Bray runtime catches it at that boundary and applies the program-root panic behavior for that run instead of returning normally through the foreign ABI.

A callable that wants to report failure to a foreign caller catches panics explicitly and returns an ABI-representable error value.

```bray
@layout(c)
struct Status
{
    code: i32;
}

@symbol(name = "bray_parse")
@abi(c)
func parse_entry(pos text: RawPointer<u8>) -> Status
{
    let result = catch
    {
        parse_foreign_text(text);
        yield Status(code = 0);
    };

    match result
    {
        case Result.Ok(status) => yield status;
        case Result.Error(_) => yield Status(code = 1);
    }
}
```

## Foreign callbacks

A foreign callback type is an ABI-qualified callable type.

```bray
callable VisitCallback =
    @abi(c) func(pos context: RawPointer<u8>, pos value: i32) -> i32;
```

A named function declaration with the same ABI can satisfy the callback type.

Captured state is not part of a plain foreign function pointer representation.

Foreign callback APIs that need state use an explicit context pointer or an ABI-laid-out context product.

```bray
@layout(c)
struct CallbackPair
{
    context: RawPointer<u8>;
    call: VisitCallback;
}
```

A lambda can satisfy an ABI-qualified callable type only when the lambda expression explicitly carries the same `@abi(...)` directive and the selected ABI permits the required callable representation.

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [Callable ABI](callable-abi.md)
- Next: [Raw pointer type](raw-pointer-type.md)
