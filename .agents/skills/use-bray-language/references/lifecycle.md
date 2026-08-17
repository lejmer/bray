# Lifecycle

**Specification:** [Lifecycle](https://github.com/lejmer/bray/blob/develop/docs/language/lifecycle.md)

## Contents

- [Lifecycle model and order](#lifecycle-model-and-order)
- [Construction](#construction)
- [Finalization and destruction](#finalization-and-destruction)
- [Scoped use](#scoped-use)
- [Lifecycle requirements in traits](#lifecycle-requirements-in-traits)
- [Partial values and replacement](#partial-values-and-replacement)
- [Scope exit, cancellation, and static cleanup](#scope-exit-cancellation-and-static-cleanup)
- [Choose the lifecycle mechanism by intent](#choose-the-lifecycle-mechanism-by-intent)

## Lifecycle model and order

**Core model:** A fully initialized owned value carries the construction, finalization, destruction, represented-part, scoped-capability, and dependency obligations declared by its concrete type. Bray tracks those obligations through moves, partial states, control flow, panic, cancellation, and static ownership so each live obligation is transferred or resolved exactly once.

When every lifecycle kind applies, the order is:

```text
construct -> ordinary use -> enter -> with body -> exit -> finalize -> destruct -> represented-part destruction
```

Only applicable stages run. `enter` and `exit` belong to `with`, finalization belongs to values with finalization obligations, destruction belongs to fully initialized values with destruction behavior, and represented-part destruction resolves remaining initialized fields or the active union payload.

Lifecycle declarations can live in an eligible type body or inherent implementation. The type body and all inherent implementations share one typed slot for the primary constructor, finalizer, destructor, scope enter, and scope exit. Named constructors use ordinary type-associated names. Lifecycle declarations, ordering, contracts, execution mode, and result shape are part of public API compatibility.

## Construction

```bray
struct Connection
{
    handle: Handle;
    mut open: bool;

    construct(pos endpoint: &Endpoint) -> Result<Self, ConnectError>
    {
        let handle: Handle = try connect(endpoint);

        return Ok(
            {
                handle = handle,
                open = true,
            }
        );
    }

    construct detached(pos handle: Handle) -> Self
    {
        return
        {
            handle = handle,
            open = false,
        };
    }
}

func create_connections(pos endpoint: &Endpoint, pos handle: Handle) -> Result<ConnectionPair, ConnectError>
{
    let active: Connection = try Connection(endpoint);
    let detached: Connection = Connection.detached(handle);

    return Ok(
        {
            active = active,
            detached = detached,
        }
    );
}
```

`construct(...)` is the primary [constructor](https://github.com/lejmer/bray/blob/develop/docs/language/lifecycle/construction.md) and is selected through `Connection(...)`. `construct detached(...)` is a named constructor and is selected through `Connection.detached(...)`. A constructor is synchronous, has no receiver or `self` binding, and returns `Self` or `Result<Self, E>`.

Defaults and represented parts are initialized before the value becomes observable as a complete value. If construction exits through `Result.Error`, panic, or cancellation, Bray resolves only the temporaries, capabilities, and represented parts that were initialized before the exit.

## Finalization and destruction

```bray
struct BufferedLog
{
    mut buffer: Buffer;
}

impl BufferedLog
{
    finalize() -> Result<unit, FlushError>
    {
        return flush_buffer(&mut self.buffer);
    }
}

impl Connection
{
    async finalize() -> Result<unit, CloseError>
    {
        if self.open
        {
            try await flush_and_close(&mut self.handle);
            self.open = false;
        }

        return Ok(unit);
    }

    destruct()
    {
        if self.open
        {
            abandon_handle(self.handle);
            self.open = false;
        }
    }
}
```

A `BufferedLog` flush can fail but does not suspend, so its finalizer is synchronous. A `Connection` close can suspend, so its finalizer is asynchronous.

A [finalizer](https://github.com/lejmer/bray/blob/develop/docs/language/lifecycle/finalization.md) receives an implicit mutable `self`, can be synchronous or asynchronous, and returns `unit` or `Result<unit, E>`. It leaves the value fully initialized. Returning `Result.Error` leaves the finalization obligation unresolved, so ordinary execution must handle, transfer, or explicitly represent that obligation before ownership can end.

A [destructor](https://github.com/lejmer/bray/blob/develop/docs/language/lifecycle/destruction.md) receives an implicit consuming mutable `self`, is synchronous and infallible, and returns `unit`. It can consume or destroy represented parts. Any represented parts still initialized when it returns are then destroyed in their type-defined order.

Finalization and destruction are selected from the concrete value. A trait view does not replace the type-wide lifecycle behavior.

## Scoped use

```bray
struct Database
{
    mut state: DatabaseState;

    mut enter() -> Result<Transaction, TransactionError>
    {
        return begin_transaction(&mut self.state);
    }

    async exit(pos transaction: Transaction) -> Result<unit, TransactionError>
    {
        return await finish_transaction(transaction);
    }
}

async func update_record(pos database: &mut Database, pos record: Record) -> Result<usize, TransactionError>
{
    let count: usize = try with transaction: Transaction = database
    {
        try transaction.write(record);

        yield transaction.changed_count();
    };

    return Ok(count);
}
```

The initializer is evaluated once. The selected [`enter`](https://github.com/lejmer/bray/blob/develop/docs/language/lifecycle/scoped-use.md) declaration receives access according to its receiver mode and produces the scoped capability matched by the binding pattern. A default receiver is shared. `mut`, `consume`, and `consume mut` select mutable, consuming, and consuming-mutable entry.

`exit` has no receiver. Its single parameter receives the capability produced by the matching `enter`, and it can reach the original value only through access carried by that capability. Both declarations can be synchronous or asynchronous. `enter` requires a result clause, while `exit` can omit `-> unit`.

The `with` body is a block expression. In value-producing context it yields exactly one result, which remains pending until `exit` completes. `exit` runs on every path leaving an entered body, including normal completion, `yield`, `return`, loop control, propagation, panic, and cancellation. If `enter` fails, the pattern and body are skipped and `exit` does not run.

An irrefutable pattern can unpack the capability:

```bray
func read_snapshot(pos store: &Store) -> Snapshot
{
    return with (view, version): (&StoreView, Version) = store
    {
        yield Snapshot.from_view(view, version = version);
    };
}
```

The annotation describes the capability value produced by `enter`. It does not select the lifecycle declaration.

## Lifecycle requirements in traits

```bray
trait DurableResource<Lease>
{
    async finalize() -> Result<unit, ResourceError>;
    destruct();
    mut enter() -> Result<Lease, ResourceError>;
    async exit(pos lease: Lease);
}

trait Openable
{
    static func open(pos path: &Path) -> Result<Self, ResourceError>;
}

impl File(DurableResource<FileLease>)
{
    mut enter() -> Result<FileLease, ResourceError>
    {
        return acquire_file_lease(&mut self);
    }

    async exit(pos lease: FileLease)
    {
        await release_file_lease(lease);
    }
}
```

[Trait lifecycle requirements](https://github.com/lejmer/bray/blob/develop/docs/language/lifecycle/lifecycle-requirements-in-traits.md) end with a semicolon. Traits can require `finalize`, `destruct`, and paired `enter` plus `exit`. Constructor requirements use static callable members returning `Self` or `Result<Self, E>`.

The concrete subject supplies compatible type-wide finalization and destruction. A trait implementation can define only the `enter` and `exit` bodies required by that exact trait, as shown above.

## Partial values and replacement

```bray
struct Packet
{
    mut payload: Payload;
    receipt: Receipt;

    destruct()
    {
        record_packet_close(&self.receipt);
    }
}

func replace_payload(pos mut packet: Packet, pos replacement: Payload) -> Packet
{
    let previous: Payload = packet.payload;

    consume_payload(previous);
    packet.payload = replacement;

    return packet;
}

func take_receipt(pos packet: Packet) -> Receipt
{
    let Packet { receipt, .. } = packet;

    return receipt;
}
```

Under the [partial-value rules](https://github.com/lejmer/bray/blob/develop/docs/language/lifecycle/partial-values-and-replacement.md), moving `packet.payload` makes `packet` partial. Reinitializing that field restores a complete value before returning it. In `take_receipt`, the whole-product destructor does not run because the value is partial. Only represented parts that remain initialized are resolved.

Whole-value replacement first resolves the old complete value through finalization, destruction, and represented-part destruction, then initializes the new value at that access path. Union replacement does the same for the old active payload. Assigning `none` resolves the old present value before making a nullable access path absent.

## Scope exit, cancellation, and static cleanup

Every reachable scope exit must agree on ownership, initialization, borrows, capabilities, effects, task obligations, and lifecycle state. A moved value is no longer resolved by its old scope. A partial value resolves only initialized represented parts.

Async block exit first broadcasts cancellation to unresolved tasks owned by that boundary. Lifecycle resolution then waits according to dependency order before storage and capabilities used by those tasks are released.

Panic and cancellation preserve ordinary lifecycle order. Cleanup that can suspend runs in a cancellation-shielded context. If fallible finalization fails during abnormal cleanup, Bray records a suppressed cleanup incident and continues through the synchronous destructor and represented-part destruction fallback. Ordinary source-level exit must resolve the finalization error instead.

[Product and thread-local statics](https://github.com/lejmer/bray/blob/develop/docs/language/declarations/static-storage-declarations.md#static-lifecycle-state) use the same value-level order. Their product host or exact native-thread attachment resolves each materialized instance once in deterministic dependency order. A static that can be needed while another static cleans up remains live until that dependent cleanup finishes.

## Choose the lifecycle mechanism by intent

| Intent                                    | Bray surface                                                                                                                            |
|-------------------------------------------|-----------------------------------------------------------------------------------------------------------------------------------------|
| Establish a complete value                | [`construct(...) -> Self` or `Result<Self, E>`](https://github.com/lejmer/bray/blob/develop/docs/language/lifecycle/construction.md)    |
| Complete fallible or asynchronous cleanup | [`finalize`](https://github.com/lejmer/bray/blob/develop/docs/language/lifecycle/finalization.md)                                       |
| Perform universal synchronous teardown    | [`destruct`](https://github.com/lejmer/bray/blob/develop/docs/language/lifecycle/destruction.md)                                        |
| Grant temporary scoped authority          | [`enter` plus `exit`](https://github.com/lejmer/bray/blob/develop/docs/language/lifecycle/scoped-use.md)                                |
| Use a scoped capability                   | [`with pattern = initializer { ... }`](https://github.com/lejmer/bray/blob/develop/docs/language/lifecycle/with-expressions.md)         |
| Require lifecycle behavior generically    | [Trait lifecycle requirements](https://github.com/lejmer/bray/blob/develop/docs/language/lifecycle/lifecycle-requirements-in-traits.md) |
| Move one represented part                 | [Partial values](https://github.com/lejmer/bray/blob/develop/docs/language/lifecycle/partial-values-and-replacement.md)                 |

**Remember:** Construct complete values, finalize obligations that can fail or suspend, destruct synchronously and infallibly, pair scoped entry with exit, and run whole-value lifecycle behavior only while the value is fully initialized.
