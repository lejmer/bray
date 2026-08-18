# Foreign data and symbols

Foreign data interoperability composes ordinary static declarations, raw pointers, symbol directives, link dependencies,
trusted memory operations, and dependency roots. It introduces no separate data declaration kind or raw code-address
type.

## Extern static declarations

`extern` means that a declaration's runtime definition or storage is supplied outside that Bray declaration. Applied to
a static, it declares provider-owned address-bearing storage.

```bray
@link(name = "c")
@symbol(name = "errno")
@thread_local
extern trusted static mut errno: std.ffi.c.int;

@link(name = "native")
@symbol(name = "native_build_id")
extern trusted static BUILD_ID: std.ffi.c.uint;
```

An extern static has a required stored type and no initializer. It is valid only at module level. It cannot have generic
parameters or `with(...)` constraints because one native symbol identifies one storage definition rather than an open
family of Bray instances.

An extern static that names a foreign ABI symbol is `trusted` and belongs to a trusted module. The declaration asserts
that the selected symbol, storage type, target identity, provider, and mutability contract agree with the native
definition.

`@thread_local` selects provider-owned thread-local storage. Resolving that declaration on one attached native thread
produces the address for that exact attachment. Its dependency contract retains both the provider and the exact
attachment.

## Storage provenance and source access

An ordinary static owns Bray storage. An extern static names provider-owned storage.

Referencing an extern static produces a `RawPointer<T>` to its storage rather than reading a `T` or creating a Bray
borrow. The reference performs symbol-address resolution only. It does not read, initialize, copy, move, borrow, or
validate the reached storage.

```bray
let address: RawPointer<std.ffi.c.int> = errno;
```

The resulting pointer carries the declaration's provider dependency. A thread-local result also carries its exact
native-thread attachment dependency. Moving or copying the pointer preserves those semantic dependencies even though
`RawPointer<T>` has no source lifetime parameter.

An extern static declaration establishes no automatic validity, initialization, aliasing, synchronization, or borrow
guarantee. Reading, writing, or converting its address to a Bray borrow uses the ordinary trusted memory operations and
must establish their exact predicates. Safe wrappers retain the provider or scoped capability as the dependency root.

`mut` states that the native storage contract permits mutation when the caller independently establishes suitable write,
initialization, alias, synchronization, and lifetime authority. It never grants ambient mutation authority and never
makes `&mut name` valid by itself. Omitting `mut` rejects write use through that declaration's symbol contract.

An extern static has no Bray initializer, finalizer, destructor, or represented-part cleanup. Its provider owns those
operations. The stored type must therefore be a foreign-storage-compatible representation or an incomplete type used
only through its address.

## Exported Bray storage

`@symbol(...)` on an ordinary non-generic static publishes that Bray-owned storage as a native data symbol.

```bray
@symbol(name = "bray_abi_version")
static ABI_VERSION: std.ffi.c.uint = std.ffi.c.uint(1);

@symbol(name = "bray_flags")
trusted static mut ABI_FLAGS: std.ffi.c.uint = std.ffi.c.uint(0);
```

The static remains owned, initialized, retained, and cleaned by its Bray product. Native symbol visibility does not
create another storage instance or change its canonical static identity.

An exported static has a complete target-supported ABI data representation. Generic statics cannot carry `@symbol(...)`
because one external symbol cannot identify multiple closed substitutions.

An immutable exported static keeps its ordinary Bray access surface. Foreign mutation violates its ABI contract.

An exported `static mut` declares storage that foreign code may mutate. It is valid only as a trusted native-symbol
declaration in a trusted module, and source references produce `RawPointer<T>` rather than an ordinary safe access path.
Bray code uses the same trusted raw-memory and synchronization rules as any other externally mutable storage.
Interior-mutation types remain the ordinary choice for mutable product statics that are not exposed as native data
symbols.

Foreign entry closes before product-static cleanup. A provider product remains loaded while any external entry,
callback, code pointer, data pointer, borrow, or owned dynamic-symbol view retains it.

## Symbol identity and availability

`@symbol(...)` describes the target symbol identity and resolution policy for callable and data declarations.

Exactly one of these identity options is supplied:

- `name = "..."` selects an exact target symbol name
- `ordinal = N` selects a target-supported symbol ordinal

The optional standard options are:

- `version = "..."`
- `binding = strong` or `binding = weak`
- `presence = required` or `presence = optional`

The defaults are strong binding and required presence. A required symbol that cannot be resolved fails product
formation. An optional extern static resolves to a null `RawPointer<T>` when absent. Address use checks
`core.memory.is_null` or uses an ordinary wrapper that performs the same check before access.

Weak binding controls target link selection. Optional presence controls whether an unresolved symbol is accepted. They
are independent contracts.

The selected target profile validates which names, ordinals, versions, bindings, presence modes, import forms, export
forms, and combinations are available. `presence = optional` is available for imported data symbols, whose raw-pointer
access can represent absence. Imported callable declarations use required presence, while optional dynamically loaded
code uses a raw callable pointer or `DynamicSymbol<F>`. An export always defines its symbol.

## Dynamic data and code symbols

Dynamic lookup produces raw pointers whose dependency contracts retain the exact dynamic-library owner. A data lookup
for `T` produces `RawPointer<T>`. A code lookup for an ABI-qualified callable type `F` produces `RawPointer<F>`.

`RawPointer<F>` is an address intended to designate code with callable contract `F`. It is not callable. The trusted
`core.memory.callable_from_pointer<F>` operation requires a non-null executable address, the exact ABI contract, target
representation support, and a live provider dependency.

```bray
callable Transform = @abi(c) func(pos value: i32) -> i32;

trusted func activate_transform(pos address: RawPointer<Transform>) -> Transform
    requires(trusted core.memory.callable_address_valid<Transform>(pointer = address))
    uses(layout_reinterpret)
{
    return trusted core.memory.callable_from_pointer<Transform>(address);
}
```

The resulting callable preserves every dependency carried by the pointer. A standard-library `DynamicSymbol<T>` owner
can wrap the pointer or callable while retaining the library owner and preventing unload.

`core.memory.pointer_from_callable<F>` exposes the raw code address of an ABI-qualified capture-free callable when the
target supports that representation. It preserves the callable's provider dependency. Data-memory operations, element
offsets, and dereference syntax remain unavailable for callable pointees.

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [Extern declarations and FFI](extern-declarations-and-ffi.md)
- Next: [Raw pointer type](raw-pointer-type.md)
