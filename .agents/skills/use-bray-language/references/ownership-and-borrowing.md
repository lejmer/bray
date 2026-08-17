# Ownership and Borrowing

**Specification:** [Ownership and borrowing](https://github.com/lejmer/bray/blob/develop/docs/language/ownership-and-borrowing.md)

## Contents

- [Ownership operations and states](#ownership-operations-and-states)
- [Borrowing and access paths](#borrowing-and-access-paths)
- [Reborrowing and nested borrow layers](#reborrowing-and-nested-borrow-layers)
- [Partial moves and reinitialization](#partial-moves-and-reinitialization)
- [Dependency contracts and ownership boundaries](#dependency-contracts-and-ownership-boundaries)
- [Choose the ownership operation by intent](#choose-the-ownership-operation-by-intent)

## Ownership operations and states

**Core model:** An owner is responsible for every initialized subvalue, dependency, capability, finalization duty, and destruction duty carried by a value, while a borrow grants temporary non-owning access to reached storage under an inferred capability and lifetime contract.

A destination, parameter, pattern, receiver, or iteration mode selects the ownership operation. The following fragments assume the named support types and callables exist.

### Context-selected moves and copies

```bray
module ownership_example;

@copy
struct Coordinate
{
    x: i32;
    y: i32;
}

struct Parcel
{
    mut contents: Contents;
    receipt: Receipt;
}

func transfer(pos parcel: Parcel, pos coordinate: Coordinate)
{
    let delivered: Parcel = parcel;
    let copied: Coordinate = coordinate;

    inspect_coordinate(coordinate);
    inspect_coordinate(copied);
    deliver(delivered);
}
```

Initializing `delivered` moves the non-copyable `Parcel` and leaves `parcel` moved from. Initializing `copied` copies the `Coordinate`, so `coordinate` remains usable. A move transfers all obligations carried by the value. A copy is available only through the type's copy contract.

### Owned, shared, mutable, and consuming call surfaces

```bray
func accept_owned(pos parcel: Parcel)
{
    deliver(parcel);
}

func observe_shared(pos parcel: &Parcel)
{
    inspect_contents(&parcel.contents);
}

func replace_contents(pos parcel: &mut Parcel, pos contents: Contents)
{
    parcel.contents = contents;
}

impl Parcel
{
    consume func unpack() -> Contents
    {
        return self.contents;
    }

    consume mut func prepare() -> Self
    {
        self.contents.prepare();

        return self;
    }
}

func call_modes(pos mut parcel: Parcel, pos replacement: Contents) -> Contents
{
    observe_shared(&parcel);
    replace_contents(&mut parcel, replacement);

    let prepared: Parcel = parcel.prepare();

    return prepared.unpack();
}
```

A plain parameter receives ownership. `&T` observes reached storage, `&mut T` grants temporary exclusive mutation authority, and a consuming receiver ends ordinary use through the old receiver path. `mut` before an owned parameter or binding grants local mutation authority. `mut` after `&` belongs to that borrow layer. Field mutation also requires every reached declaration and type-form layer to permit it.

### Explicit consuming contexts

```bray
union Message
{
    Data(pos payload: Payload);
    Empty;
}

func take_payload(pos message: Message) -> Payload
{
    return match consume message
    {
        case Data(payload) { yield payload; }
        case Empty { yield Payload.empty(); }
    };
}

func iteration_modes(
    pos shared: Items,
    pos mut mutable: Items,
    pos owned: Items,
)
{
    for item in shared
    {
        observe_item(item);
    }

    for item in mut mutable
    {
        update_item(item);
    }

    for item in move owned
    {
        consume_item(item);
    }
}
```

`match consume` lets the selected arm move fields or payloads from an owned subject. Unmarked iteration borrows its source for shared access, `in mut` mutably borrows it, and `in move` moves it into the cursor for consuming iteration.

An owned access path can be uninitialized, partially initialized, fully initialized, moved from, or destroyed. Only operations supported by the current state, type, capability, dependencies, and lifecycle contract are valid. Scope exit destroys only values and subvalues still owned and initialized in that scope.

## Borrowing and access paths

An [access path](https://github.com/lejmer/bray/blob/develop/docs/language/ownership-and-borrowing/storage-and-access-paths.md) reaches storage through a binding, field, tuple element, active payload, indexed element, slice range, static, borrow, or another type-form projection. Compatibility is decided by reached storage rather than spelling.

```bray
struct Counter
{
    mut value: i64;
}

struct CounterPair
{
    mut left: Counter;
    mut right: Counter;
}

func increment(pos counter: &mut Counter)
{
    counter.value += 1;
}

func borrow_pair(pos mut pair: CounterPair)
{
    let shared: &CounterPair = &pair;
    let copied_shared: &CounterPair = shared;

    inspect_pair(shared);
    inspect_pair(copied_shared);

    let left: &mut Counter = &mut pair.left;
    let right: &mut Counter = &mut pair.right;

    increment(left);
    increment(left);
    increment(right);

    let transferred: &mut Counter = left;

    increment(transferred);
}
```

Shared borrows can be copied and can coexist when their capabilities are compatible. Mutable borrows are not copyable. The two field borrows coexist because the compiler proves `left` and `right` disjoint. Passing `left` repeatedly to a mutable-borrow parameter reborrows it for each call. Assigning it to `transferred` moves the borrow value and its temporary mutation authority, not the reached `Counter`.

[Borrowing](https://github.com/lejmer/bray/blob/develop/docs/language/ownership-and-borrowing/borrow-rules.md) suspends conflicting observation, mutation, movement, replacement, reinitialization, finalization, and destruction of the reached storage until the borrow's last required use. If overlap cannot be proven, Bray treats access paths as conflicting.

## Reborrowing and nested borrow layers

A reborrow derives no more authority than its source and temporarily suspends incompatible use of the original borrow. Every nested borrow layer has its own capability.

```bray
func borrow_layers(
    pos shared: &Counter,
    pos mut exclusive: &mut Counter,
)
{
    let mut shared_slot: &Counter = shared;

    let shared_shared: &&Counter = &shared_slot;
    let shared_mutable: &&mut Counter = &exclusive;

    inspect_outer(shared_shared, shared_mutable);

    let mutable_shared: &mut &Counter = &mut shared_slot;
    let mutable_mutable: &mut &mut Counter = &mut exclusive;

    update_outer(mutable_shared, mutable_mutable);
}
```

`&&T` shares a shared-borrow value. `&&mut T` shares a mutable-borrow value without gaining mutation authority over `T`. `&mut &T` can replace or mutate the stored shared-borrow value. `&mut &mut T` can replace or mutate the stored mutable-borrow value. Reachable operations follow the capabilities of the complete path through every layer.

A stored borrow never extends the reached storage lifetime. Returning or storing one is valid only when the inferred dependency contract of the destination preserves its source storage and every required capability.

## Partial moves and reinitialization

Moving one initialized subpart leaves the original owner partially initialized. Initialized remainder parts retain their own duties and are the only parts destroyed if ownership ends before reinitialization.

```bray
struct Package
{
    mut payload: Payload;
    receipt: Receipt;
}

func replace_payload(pos mut package: Package) -> Package
{
    let payload: Payload = package.payload;

    process_payload(payload);

    package.payload = Payload.empty();

    return package;
}

func take_receipt(pos package: Package) -> Receipt
{
    let Package { receipt, .. } = package;

    return receipt;
}
```

The first function moves `payload`, reinitializes that field, and restores a complete `Package` before returning it. The second moves only `receipt`. The remaining initialized fields stay under `package` and are resolved when the scope exits.

Field, payload, tuple-element, array-element, and other move-eligible projections can create partial state. A value with whole-value lifecycle behavior must be fully reinitialized before any path can observe, borrow, copy, move, consume, finalize, or destroy it as a complete value.

## Dependency contracts and ownership boundaries

Bray infers [dependency contracts](https://github.com/lejmer/bray/blob/develop/docs/language/ownership-and-borrowing/dependency-contracts.md). A contract records storage, initialization, borrow authority, exclusivity, scoped capabilities, lifecycle duties, product roots, exact-thread roots, and other conditions that must remain valid.

### Returned and stored borrows

```bray
struct Cursor
{
    data: &[u8];
    mut index: usize;
}

func same(pos value: &u8) -> &u8
{
    return value;
}

func cursor(pos data: &[u8]) -> Cursor
{
    return Cursor
    {
        data = data,
        index = 0,
    };
}

func owned_buffer() -> Buffer
{
    let buffer: Buffer = Buffer.empty();

    return buffer;
}
```

The result of `same` depends on the storage reached through `value`. A `Cursor` carries the dependency of its `data` field and cannot outlive that storage. `owned_buffer` can return its local value because ownership moves into the result and no dependency on the local binding remains.

### Product and exact-thread roots

```bray
static CONFIGURATION: Configuration = Configuration.empty();

@thread_local
static THREAD_CONTEXT: ThreadContext = ThreadContext.empty();

func configuration() -> &Configuration
{
    return &CONFIGURATION;
}

func thread_context() -> &ThreadContext
{
    return &THREAD_CONTEXT;
}
```

A product-static borrow carries its exact static and provider-product roots. A thread-local static borrow also carries the exact native-thread attachment root, cannot be used on another native thread, and pins a retained task while that dependency remains live. Static access exposes shared storage. A product-static or thread-local value cannot be moved, replaced, or directly mutably borrowed, and interior mutation requires a valid scoped capability.

### Explicit callable state

```bray
func write_message(pos mut output: File, pos message: string)
{
    let write_line = lambda (pos destination: &mut File, pos text: string)
    {
        destination.write(text);
    };

    write_line(&mut output, message);
}
```

Lambdas do not capture enclosing local state. Pass borrowed or owned context explicitly so the callable value's dependency contract remains representable.

Values cross return, yield, call, storage, async, task, trait-view, package, and export boundaries only when the destination preserves every carried dependency. Composite values preserve the transitive dependencies of their borrows, futures, tasks, callable values, trait views, boxes, and aggregates. Expected types do not create missing lifetimes or capabilities. Mutation, movement, destruction, reinitialization, finalization, variant replacement, assigning `none`, or capability loss invalidates borrows and guarantees that depend on the changed state.

## Choose the ownership operation by intent

| Intent                              | Bray surface                                                                                                                                                                                                         | Decisive rule                                                                                                |
|-------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------|
| Transfer an owned value             | [Use it in an owned destination or parameter](https://github.com/lejmer/bray/blob/develop/docs/language/ownership-and-borrowing/moves-copies-and-consumption.md)                                                     | The context moves a non-copyable value and every obligation it carries.                                      |
| Duplicate an ordinary value         | Use a type with a [copy contract](https://github.com/lejmer/bray/blob/develop/docs/language/types/copy-contracts.md) in a copy-selecting context                                                                     | Copying is implicit, infallible, and unavailable without the exact type's copy contract.                     |
| Observe existing storage            | [`&path`](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/borrow-expressions.md) or a shared-borrow parameter                                                                                  | Compatible shared borrows can coexist and copy without transferring storage ownership.                       |
| Mutate existing storage temporarily | [`&mut path`](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/borrow-expressions.md) or a mutable-borrow parameter                                                                             | The path needs mutation authority and statically compatible exclusivity for the borrow duration.             |
| Derive shorter access from a borrow | A [reborrow](https://github.com/lejmer/bray/blob/develop/docs/language/ownership-and-borrowing/reborrowing-and-borrow-values.md) through parameter passing, receiver use, projection, or another borrow              | The reborrow cannot exceed its source authority and suspends incompatible source use while active.           |
| Consume by structural case          | [`match consume value`](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/match-expressions.md)                                                                                                  | Guards observe first, then the selected arm may move matched parts.                                          |
| Iterate without consuming           | [`for pattern in source` or `for pattern in mut source`](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/iteration-source-resolution.md)                                                       | Shared and mutable iteration borrow the source until the hidden cursor is resolved.                          |
| Consume an iterable source          | [`for pattern in move source`](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/iteration-source-resolution.md)                                                                                 | The source moves into its cursor and the owned `Iterable` implementation controls produced elements.         |
| Move selected stored parts          | A field, payload, tuple, array, index, or [destructuring partial move](https://github.com/lejmer/bray/blob/develop/docs/language/ownership-and-borrowing/partial-moves.md)                                           | The containing value becomes partial until every moved part is restored or the remaining parts are resolved. |
| Return or store a borrow            | A borrow-bearing result or field whose [dependency contract](https://github.com/lejmer/bray/blob/develop/docs/language/ownership-and-borrowing/scope-exits-and-ownership-boundaries.md) reaches valid source storage | Storing a borrow never extends its source lifetime or scoped capability.                                     |
| Share ambient persistent storage    | A borrow from [`static` or `@thread_local static`](https://github.com/lejmer/bray/blob/develop/docs/language/declarations/static-storage-declarations.md)                                                            | The result carries product identity and, for thread-local storage, exact attachment identity.                |

**Remember:** Moves transfer obligations, copies require a copy contract, mutable borrows carry exclusive temporary authority, shared borrows are copyable, and every returned, stored, or transferred value must preserve its inferred storage and capability dependencies.
