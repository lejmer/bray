# Targets, Layout, ABI, and Raw Memory

**Specification:** [Targets, layout, ABI, and raw memory](https://github.com/lejmer/bray/blob/develop/docs/language/targets-layout-abi-and-raw-memory.md)

## Contents

- [Selected targets and gated source](#selected-targets-and-gated-source)
- [Physical layout and layout queries](#physical-layout-and-layout-queries)
- [Callable ABI, linking, and foreign entry](#callable-abi-linking-and-foreign-entry)
- [Raw pointers and trusted memory access](#raw-pointers-and-trusted-memory-access)
- [Protected storage, allocation, and anchored borrows](#protected-storage-allocation-and-anchored-borrows)
- [Device memory and operating-system surfaces](#device-memory-and-operating-system-surfaces)
- [Target control](#target-control)
- [Inline assembly](#inline-assembly)
- [Choose the boundary by intent](#choose-the-boundary-by-intent)

## Selected targets and gated source

**Core model:** One immutable target profile supplies every compile-time target property for a product before source selection, layout, ABI checking, memory checking, const evaluation, and code generation. `target.*` properties are ambient compiler-known constants, while `@target(...)` includes or excludes a complete module contribution.

```bray
module target_example;

const TARGET_NAME: string = target.identity.NAME;
const POINTER_BYTES: usize = target.pointer.BYTES;
const LITTLE_ENDIAN: bool = target.endian.LITTLE;
const C_ABI_AVAILABLE: bool = target.abi.C;
const NATIVE_THREADS_AVAILABLE: bool = target.platform.native_threads;

@target(target.atomic.U64 && target.atomic.U64_ALWAYS_LOCK_FREE)
module counter_backend
{
    func add(pos counter: &std.atomic.Atomic<u64>, amount: u64) -> u64
    {
        return std.atomic.fetch_add(
            counter,
            amount,
            order = std.atomic.ReadModifyWriteOrder.AcquireRelease,
        );
    }
}

@target(!(target.atomic.U64 && target.atomic.U64_ALWAYS_LOCK_FREE))
module counter_backend
{
    struct Counter
    {
        mut value: u64;
    }

    func add(pos counter: &mut Counter, amount: u64) -> u64
    {
        let previous = counter.value;
        counter.value += amount;

        return previous;
    }
}

@target(target.platform.native_threads)
module thread_state
{
    @thread_local
    static CURRENT: ThreadState = ThreadState.empty();
}
```

The two `counter_backend` blocks contribute to the same module identity for disjoint target conditions. A disabled contribution is absent for that product, which lets its body mention declarations unavailable on the selected target. Gating does not alter visibility, trust, module identity, or runtime control flow.

Target properties cover identity, pointer width, scalar support, endianness, alignment, callable ABI, exact C scalar mappings, atomics, address spaces, allocation, platform services, and linkage. Public signatures, constants, layouts, implementations, and availability that depend on them retain the relevant target dependencies in compiled interfaces.

See [target profiles and properties](https://github.com/lejmer/bray/blob/develop/docs/language/targets-layout-abi-and-raw-memory/target-profiles-and-properties.md), [target constraints and gates](https://github.com/lejmer/bray/blob/develop/docs/language/targets-layout-abi-and-raw-memory/target-constraints-and-gates.md), and [static storage declarations](https://github.com/lejmer/bray/blob/develop/docs/language/declarations/static-storage-declarations.md).

## Physical layout and layout queries

Default product and union layout preserves Bray semantics but makes no public physical-layout promise. `@layout(stable)` fixes Bray-defined target layout, `@layout(c)` selects the target C data layout, and `@layout(transparent)` gives a one-storage-field product its field's layout and ABI.

```bray
@copy
@layout(stable, align = 16)
struct Vector
{
    x: r32;
    y: r32;
    z: r32;
    w: r32;
}

@copy
@layout(stable, pack = 1)
struct WireHeader
{
    kind: u8;
    length: u32;
}

@copy
@layout(c)
struct NativePoint
{
    x: r64;
    y: r64;
}

@copy
@layout(transparent)
struct FileDescriptor
{
    internal value: i32;
}

@copy
@layout(stable, tag = u8)
union Packet
{
    @tag(1)
    Empty;

    @tag(2)
    Bytes(pos value: [u8; 16]);
}

@copy
@layout(c, tag = u32)
union NativeStatus
{
    @tag(0)
    Ok;

    @tag(1)
    Error(pos code: u32);
}

const VECTOR_BYTES: usize = std.memory.size_of<Vector>();
const VECTOR_ALIGNMENT: usize = std.memory.align_of<Vector>();
const VECTOR_STRIDE: usize = std.memory.stride_of<Vector>();

func allocation_layout(count: usize) -> Result<std.memory.MemoryLayout, std.memory.MemoryLayoutError>
{
    return std.memory.layout_of<Vector>(count = count);
}
```

`align` raises aggregate alignment. `pack` caps alignment only for stable plain storage, and packed components cannot be borrowed unless the access is naturally aligned. A laid-out union may select its integer tag representation, and explicit variant tags must be unique and complete. Padding is never a semantic value or implicitly initialized storage.

The layout helpers describe size, alignment, array stride, and checked allocation layout for the selected target. They do not read, initialize, allocate, or grant access to memory. An explicit layout stabilizes representation only according to that contract. It does not change ownership, lifecycle, field semantics, or callable ABI.

See [layout contracts](https://github.com/lejmer/bray/blob/develop/docs/language/targets-layout-abi-and-raw-memory/layout-contracts.md), [product layout](https://github.com/lejmer/bray/blob/develop/docs/language/targets-layout-abi-and-raw-memory/product-layout.md), [union layout](https://github.com/lejmer/bray/blob/develop/docs/language/targets-layout-abi-and-raw-memory/union-layout.md), and [layout helpers](https://github.com/lejmer/bray/blob/develop/docs/language/targets-layout-abi-and-raw-memory/layout-helpers.md).

## Callable ABI, linking, and foreign entry

`@abi(c)` and `@abi(system)` select callable contracts, not data layout. A foreign callable boundary uses ABI-representable values, an exact external symbol, a declared link dependency, and explicit `foreign_call` trust. An exported Bray function has a body and is not `extern`.

```bray
@link(name = "native_math", kind = dynamic)
trusted module native_math
{
    @copy
    @layout(c)
    struct Point
    {
        x: r64;
        y: r64;
    }

    callable Transform = @abi(c)
        func(pos context: RawPointer<u8>, pos point: Point) -> Point;

    @symbol(name = "native_transform")
    @abi(c)
    extern trusted func transform(pos callback: Transform, pos context: RawPointer<u8>, pos point: Point) -> Point
        uses(foreign_call);

    @symbol(name = "bray_translate")
    @abi(c)
    func translate(pos context: RawPointer<u8>, pos point: Point) -> Point
    {
        return Point(x = point.x + 1.0, y = point.y + 1.0);
    }

    @symbol(name = "native_platform_tick")
    @abi(system)
    extern trusted func platform_tick() -> unit
        uses(foreign_call);
}
```

An `extern` declaration introduces a linked callable surface and ends with a semicolon. `@link` selects an artifact already supplied by the build graph, while `@symbol` identifies the imported or exported native symbol. A foreign callback is an ABI-qualified capture-free callable. Pass state as an ABI-laid-out context product, or own stable-address state with `std.ffi.CallbackContext<State>`, pass its opaque context pointer, and borrow the state inside the matching exported entry through trusted `std.ffi.callback_state<State>(context)`.

By-value foreign parameters and results are restricted to representations accepted by the selected ABI. Typical accepted forms are scalars, unit results, raw pointers, same-ABI callables, `@layout(c)` aggregates, and compatible transparent wrappers. Borrows, slices, trait views, boxes, tasks, async computations, default-layout aggregates, and default-ABI callables need an explicit boundary representation.

Panic and cancellation do not unwind through foreign frames. Pointer validity, ownership, thread affinity, callback lifetime, reentrancy, and resource state required by the foreign API belong in the declaration's types and contract.

See [callable ABI](https://github.com/lejmer/bray/blob/develop/docs/language/targets-layout-abi-and-raw-memory/callable-abi.md) and [extern declarations and FFI](https://github.com/lejmer/bray/blob/develop/docs/language/targets-layout-abi-and-raw-memory/extern-declarations-and-ffi.md).

## Raw pointers and trusted memory access

`RawPointer<T>` is a copyable address value without ownership, lifetime, validity, alignment, initialization, aliasing, or provenance guarantees. Raw access succeeds only when the declaration contract proves the relevant predicates and the implementation acknowledges the required capabilities.

```bray
trusted module raw_example;

trusted func duplicate_words(
    pos source: RawPointer<u32>,
    pos destination: RawPointer<u32>,
    count: usize,
) -> (u32, RawPointer<u8>)
    requires(
        count > 0,
        trusted core.memory.valid_read<u32>(pointer = source, count = count),
        trusted core.memory.valid_read<u32>(pointer = destination, count = count),
        trusted core.memory.valid_write<u32>(pointer = destination, count = count),
        trusted core.memory.aligned_for<u32>(pointer = source),
        trusted core.memory.aligned_for<u32>(pointer = destination),
        trusted core.memory.initialized_range_as<u32>(pointer = source, count = count),
        trusted core.memory.non_overlapping<u32>(
            left = source,
            left_count = count,
            right = destination,
            right_count = count,
        ),
    )
    uses(layout_reinterpret, raw_memory, unchecked_init)
{
    trusted core.memory.copy<u32>(source = source, destination = destination, count = count);

    let first: u32 = trusted core.memory.read<u32>(destination);
    let next: RawPointer<u32> = core.memory.offset<u32>(destination, elements = 1);
    let bytes: RawPointer<u8> = trusted core.memory.reinterpret<u8, u32>(next);
    let following: RawPointer<u8> = core.memory.byte_offset<u8>(bytes, bytes = 4);

    return(first, following);
}

trusted func initialize_byte(pos pointer: RawPointer<u8>, value: u8)
    requires(
        trusted core.memory.valid_write<u8>(pointer = pointer, count = 1),
        trusted core.memory.aligned_for<u8>(pointer = pointer),
    )
    ensures(trusted core.memory.initialized_as<u8>(pointer = pointer))
    uses(raw_memory, unchecked_init)
{
    trusted core.memory.write<u8>(pointer, value);
}

trusted func shift_words(pos source: RawPointer<u32>, pos destination: RawPointer<u32>, count: usize)
    requires(
        trusted core.memory.valid_read<u32>(pointer = source, count = count),
        trusted core.memory.valid_write<u32>(pointer = destination, count = count),
        trusted core.memory.initialized_range_as<u32>(pointer = source, count = count),
    )
    ensures(trusted core.memory.initialized_range_as<u32>(pointer = destination, count = count))
    uses(raw_memory, unchecked_init)
{
    trusted core.memory.copy_overlapping<u32>(source = source, destination = destination, count = count);
}

func observe_references(pos value: &u32, pos mutable: &mut u32) -> (RawPointer<u32>, RawPointer<u32>, bool)
{
    let pointer: RawPointer<u32> = core.memory.address_of<u32>(value);
    let mutable_pointer: RawPointer<u32> = core.memory.address_of_mut<u32>(mutable);
    let null: RawPointer<u32> = core.memory.null<u32>();

    return(pointer, mutable_pointer, core.memory.is_null<u32>(null));
}
```

`read` moves a non-copyable value out and invalidates its initialization state, while reading a copyable value preserves it. `write` establishes initialization only after completion. `copy` requires non-overlap, `copy_overlapping` permits overlap, and both copy representations without running constructors, destructors, finalizers, or copy behavior.

Pointer `offset` and `byte_offset` are explicit operations. Reinterpretation changes the typed pointer view but establishes no validity or initialization. A raw pointer has no source dereference, indexing, field-access, or arithmetic syntax, and integer conversion uses the explicit target-address operations described below.

See the [raw pointer type](https://github.com/lejmer/bray/blob/develop/docs/language/targets-layout-abi-and-raw-memory/raw-pointer-type.md), [core memory declarations](https://github.com/lejmer/bray/blob/develop/docs/language/targets-layout-abi-and-raw-memory/core-memory-declarations.md), [raw memory access and copying](https://github.com/lejmer/bray/blob/develop/docs/language/targets-layout-abi-and-raw-memory/raw-memory-access-and-copying.md), and [raw memory predicates and capabilities](https://github.com/lejmer/bray/blob/develop/docs/language/targets-layout-abi-and-raw-memory/raw-memory-predicates-and-capabilities.md).

## Protected storage, allocation, and anchored borrows

`Uninit<T>` owns protected storage with `T`'s size and alignment without creating a `T`. `std.memory.Output<T>` and `InPlace<T>` provide safe one-step initialization, while trusted extraction and raw-to-borrow conversion require exact state and authority proofs.

```bray
func construct_output<T>(pos value: T) -> T
{
    let mut output: std.memory.Output<T> = std.memory.Output<T>();

    return consume output.write(value);
}

trusted func construct_in_place<T>(pos value: T) -> T
    uses(unchecked_init)
{
    let mut storage: Uninit<T> = std.memory.uninit<T>();

    {
        let mut placement: std.memory.InPlace<T> = std.memory.in_place<T>(&mut storage);
        let _: &mut T = placement.write(value);
    };

    return trusted std.memory.assume_initialized<T>(storage);
}

trusted func anchored_read<T, Owner>(
    pos owner: &Owner,
    pos pointer: RawPointer<T>,
) -> std.memory.AnchoredView<T, Owner>
    requires(
        trusted core.memory.valid_read<T>(pointer = pointer, count = 1),
        trusted core.memory.aligned_for<T>(pointer = pointer),
        trusted core.memory.initialized_as<T>(pointer = pointer),
        trusted core.memory.shared_alias_valid<T>(pointer = pointer),
        trusted core.memory.epoch_current<T>(pointer = pointer),
        trusted core.memory.synchronized_access<T>(pointer = pointer),
        trusted core.memory.movement_stable<T>(pointer = pointer),
        trusted core.memory.finalization_pending<T>(pointer = pointer),
    )
{
    return trusted std.memory.anchored_view<T, Owner>(owner, pointer);
}

trusted func anchored_write<T, Capability>(
    pos capability: &mut Capability,
    pos pointer: RawPointer<T>,
) -> std.memory.AnchoredViewMut<T, Capability>
    requires(
        trusted core.memory.valid_write<T>(pointer = pointer, count = 1),
        trusted core.memory.aligned_for<T>(pointer = pointer),
        trusted core.memory.initialized_as<T>(pointer = pointer),
        trusted core.memory.exclusive_alias_valid<T>(pointer = pointer),
        trusted core.memory.epoch_current<T>(pointer = pointer),
        trusted core.memory.synchronized_access<T>(pointer = pointer),
        trusted core.memory.movement_stable<T>(pointer = pointer),
        trusted core.memory.finalization_pending<T>(pointer = pointer),
    )
{
    return trusted std.memory.anchored_view_mut<T, Capability>(capability, pointer);
}

trusted func allocate_words(count: usize) -> Result<std.memory.RawBuffer<u32>, std.memory.MemoryLayoutError>
    uses(layout_reinterpret, manual_alloc)
{
    return trusted std.memory.RawBuffer<u32>(capacity = count);
}
```

An anchored borrow retains the exact owner or scoped capability as its dependency root. Shared conversion requires shared alias validity, while mutable conversion requires exclusive alias validity and mutable capability authority. Both also require a current epoch, synchronization, movement stability, and pending finalization.

`core.memory.allocate` returns owned writable storage or panics, and deallocation requires the exact pointer, byte count, alignment, and ownership condition. `RawAllocation` is the untyped linear owner. `RawBuffer<T>` is a linear typed-storage owner with a checked initialized prefix. Moving either owner transfers its obligations, while copying an exposed pointer does not.

See [manual allocation](https://github.com/lejmer/bray/blob/develop/docs/language/targets-layout-abi-and-raw-memory/manual-allocation.md), [uninitialized storage and anchored borrows](https://github.com/lejmer/bray/blob/develop/docs/language/targets-layout-abi-and-raw-memory/uninitialized-storage-and-anchored-borrows.md), [the standard-library memory surface](https://github.com/lejmer/bray/blob/develop/docs/language/targets-layout-abi-and-raw-memory/standard-library-memory-surface.md), and [raw allocation and buffers](https://github.com/lejmer/bray/blob/develop/docs/language/targets-layout-abi-and-raw-memory/raw-allocation-and-buffers.md).

## Device memory and operating-system surfaces

Host `RawPointer<T>` and target-address-space `DevicePointer<T>` are distinct protected types. Device memory is not host-accessible until a recognized mapping contract establishes scoped host validity, alignment, initialization, synchronization, and access conditions.

```bray
module target_surfaces;

@target(target.address_space.DEVICE)
trusted module device_access
{
    trusted func load_status(pos pointer: DevicePointer<u32>) -> u32
        requires(
            trusted core.target.device_valid_read<u32>(pointer = pointer, count = 1),
            trusted core.target.device_aligned_for<u32>(pointer = pointer),
            trusted core.target.device_initialized_as<u32>(pointer = pointer),
        )
        uses(device_memory, raw_memory)
    {
        return trusted core.target.device_volatile_load<u32>(pointer);
    }

    trusted func store_status(pos pointer: DevicePointer<u32>, value: u32)
        requires(
            trusted core.target.device_valid_write<u32>(pointer = pointer, count = 1),
            trusted core.target.device_aligned_for<u32>(pointer = pointer),
        )
        ensures(trusted core.target.device_initialized_as<u32>(pointer = pointer))
        uses(device_memory, raw_memory, unchecked_init)
    {
        trusted core.target.device_volatile_store<u32>(pointer, value);
    }
}

@target(target.identity.SYSTEM == "windows")
module platform_handle
{
    func retain(pos handle: &std.os.windows.Handle)
    {
        use_handle(handle);
    }
}
```

Device owners, buffers, transfers, synchronization, and mappings use `device_memory` and any additional host-memory or foreign-call capabilities their contracts require. Volatile device access is not atomic and creates no synchronization edge.

Target-specific operating-system modules are `std.os.windows`, `std.os.linux`, and `std.os.darwin`. Their handle, descriptor, owner, and resource types remain distinct even when their underlying integer widths match. Portable code should prefer the corresponding portable standard-library service.

See [device memory](https://github.com/lejmer/bray/blob/develop/docs/language/targets-layout-abi-and-raw-memory/device-memory.md) and [target-specific operating-system modules](https://github.com/lejmer/bray/blob/develop/docs/language/targets-layout-abi-and-raw-memory/target-specific-operating-system-modules.md).

## Target control

Target-control declarations make volatile access, address exposure, fences, termination, spin hints, and compile-time instruction-feature checks explicit. They are trusted because their optimizer, hardware, provenance, or control-flow effects are not ordinary Bray operations.

```bray
trusted module target_control;

trusted func observe_register(pos pointer: RawPointer<u32>) -> (u32, usize)
    requires(
        trusted core.memory.valid_read<u32>(pointer = pointer, count = 1),
        trusted core.memory.aligned_for<u32>(pointer = pointer),
        trusted core.memory.initialized_as<u32>(pointer = pointer),
    )
    uses(intrinsic, raw_memory)
{
    let value: u32 = trusted core.target.volatile_load<u32>(pointer);
    let address: usize = trusted core.target.expose_address<u32>(pointer);
    let reconstructed: RawPointer<u32> = trusted core.target.from_exposed_address<u32>(address = address);

    assert(trusted core.target.address_equal<u32>(pointer, reconstructed));

    let _: bool = trusted core.target.address_less<u32>(core.memory.null<u32>(), pointer);

    trusted core.target.compiler_fence(MemoryOrder.Release);
    trusted core.target.hardware_fence(MemoryOrder.AcquireRelease);
    trusted core.target.spin_loop_hint();

    let _: bool = trusted core.target.feature_enabled("sse2");

    return(value, address);
}

trusted func update_register(pos pointer: RawPointer<u32>, value: u32)
    requires(
        trusted core.memory.valid_write<u32>(pointer = pointer, count = 1),
        trusted core.memory.aligned_for<u32>(pointer = pointer),
    )
    ensures(trusted core.memory.initialized_as<u32>(pointer = pointer))
    uses(intrinsic, raw_memory, unchecked_init)
{
    trusted core.target.volatile_store<u32>(pointer, value);
    trusted core.target.debugger_trap();
}

trusted func stop_now() -> never
    uses(intrinsic)
{
    trusted core.target.abort();
}

trusted func impossible() -> never
    requires(false)
    uses(intrinsic)
{
    trusted core.target.unreachable();
}
```

Address exposure discards provenance. Reconstruction restores only an address-shaped raw pointer and establishes none of the conditions required for access. `address_equal` and `address_less` compare target-width exposed addresses, not provenance or allocation identity.

A compiler fence restricts optimizer movement but creates no hardware synchronization edge. A hardware fence emits target synchronization. Both reject `Relaxed`. `spin_loop_hint` neither yields a task nor observes cancellation. `abort` skips cleanup and propagation, `debugger_trap` may continue after debugger resumption, and `unreachable` terminates a violated trusted reachability claim.

## Inline assembly

Inline assembly is a checked trusted target operation with typed tuple inputs and outputs, literal templates and descriptors, target-validated constraints and clobbers, and explicit continuing, diverging, or branching control flow. A one-element input, output, or label tuple keeps its trailing comma.

```bray
trusted module assembly_example;

func alternate() -> never
{
    loop {}
}

trusted func increment(pos value: i32) -> i32
    uses(device_memory, intrinsic, raw_memory, unchecked_alias, unchecked_init)
{
    let output: (i32, ) = trusted core.target.assembly<(i32, ), (i32, )>(
        template = "",
        constraints = "+reg",
        clobbers = "",
        features = "",
        options = 1,
        inputs = (value,),
    );

    return output.0;
}

trusted func choose(pos value: i32) -> i32
    uses(device_memory, intrinsic, raw_memory, unchecked_alias, unchecked_init)
{
    let output: (i32, ) = trusted core.target.branching_assembly<(i32, ), (i32, ), (func() -> never, )>(
        template = "",
        constraints = "+reg,label",
        clobbers = "",
        features = "",
        options = 1,
        inputs = (value,),
        labels = (alternate,),
    );

    return output.0;
}

trusted func halt()
    uses(device_memory, intrinsic, raw_memory, unchecked_alias, unchecked_init)
{
    trusted core.target.diverging_assembly<(i32, )>(
        template = "ud2",
        constraints = "reg",
        clobbers = "",
        features = "",
        options = 0,
        inputs = (0,),
    );
}
```

Constraints describe late outputs with `=`, early outputs with `=&`, input-output operands with `+` or `+&`, register classes or physical registers, immediates with `i`, callable symbols with `s`, memory with `m`, and alternate targets with `label`. Output-bearing descriptors precede pure inputs and labels. Immediate and symbol operands are compile-time identities and do not occupy runtime tuple positions.

Clobbers name target registers, portable machine state such as `memory`, `cc`, or `flags`, or an ABI caller-saved set such as `abi:C`. Option bits are `1` for pure, `2` for aligned stack, and `4` for Intel syntax where supported. Bit `8` requests unwinding and is rejected because assembly cannot unwind through Bray frames. Pure assembly cannot declare a memory clobber.

Every feature, register class, physical register, clobber ABI, dialect, operand type, tuple position, and control-flow shape is validated against the selected target before MIR lowering. WebAssembly targets reject inline assembly.

See [target control and inline assembly](https://github.com/lejmer/bray/blob/develop/docs/language/targets-layout-abi-and-raw-memory/target-control-and-inline-assembly.md).

## Choose the boundary by intent

| Need                                      | Use                                                              |
|-------------------------------------------|------------------------------------------------------------------|
| Select source for a target capability     | `@target(...)` on a module contribution                          |
| Observe a target property at compile time | `target.*`                                                       |
| Stabilize Bray data layout                | `@layout(stable)`                                                |
| Match C aggregate layout                  | `@layout(c)`                                                     |
| Preserve one field's layout and ABI       | `@layout(transparent)`                                           |
| Select a foreign calling convention       | `@abi(c)` or `@abi(system)`                                      |
| Import a linked callable                  | `extern` with `@link`, `@symbol`, ABI, trust, and `foreign_call` |
| Export a Bray callable as a native symbol | A Bray body with `@symbol` and `@abi`                            |
| Observe size or allocate typed storage    | `std.memory` layout helpers and owners                           |
| Address storage without borrow semantics  | `RawPointer<T>` plus explicit operations                         |
| Build a value in protected storage        | `Uninit<T>`, `Output<T>`, or `InPlace<T>`                        |
| Borrow foreign or mapped storage          | Anchored borrow conversion with exact authority                  |
| Access a non-host address space           | `DevicePointer<T>` and `device_memory` contracts                 |
| Control optimizer or target instructions  | `core.target` operations                                         |
| Express checked machine instructions      | Typed inline assembly                                            |

**Remember:** The selected target decides availability and representation. Layout directives govern data, ABI directives govern calls, raw pointers carry no authority, trusted memory operations require explicit proofs and capabilities, anchored borrows retain exact authority, device pointers remain separate from host pointers, and target control remains explicit in source and observable behavior.
