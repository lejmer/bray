# Foreign and platform interoperability

Interoperability builds on the language's ABI, layout, raw-memory, trust, and target model. Ordinary standard-library
wrappers express ownership and policy around foreign mechanisms. The compiler checks those declared contracts rather
than inferring them from a C signature or an address.

## Module responsibilities

`std.ffi` owns shared boundary helpers and explicit callback state. `std.ffi.c` supplies C values and string adapters.
`std.dynamic` owns loaded libraries and borrowed symbols. Target-specific `std.os.*` modules expose facilities whose
representation or meaning cannot be portable. Portable files, processes, time, and threads stay in their service
modules.

C scalar wrappers use exact selected target properties, not the compiler host's widths or a similar neighboring C type.
Their transparent representation and callable classification are checked together. Borrowed and owned C strings remain
distinct, with explicit encoding and bounded raw access. These adapters reuse the existing language type system.

Native resources retain concrete types for their release operation, invalid states, affinity, and synchronization. An
integer or pointer alone is not an ownership contract. Ordinary borrowing, transfer, duplication, and lifecycle rules
apply to trusted wrappers without a universal untyped handle owner.

## Dynamic code and lifetime

Dynamic loading uses explicit paths or target-defined system-library policy. A typed symbol borrow retains its library
owner. The loader proves address existence, while the trusted wrapper remains responsible for signature, layout,
ownership, and effects. Loading a library does not alter the compiler package graph.

Loaded Bray providers also retain dependencies through callable entries, callbacks, external owners, and static
consumers. Entry closure and external-root quiescence precede cleanup and unload. Internal static dependencies order
consumer cleanup before provider cleanup. Symbol and storage release keep their provider alive until callbacks return.

Dynamic-loader mechanisms and compiler-generated callback roots have separate roles. The former belong to platform
services, while the latter reuse the runtime's synchronous-root and thread-attachment model.

## Callbacks and reentry

Bray callable values remain capture-free. Stateful callbacks pair a static ABI-qualified entry with an explicit context
owner. The wrapper declares whether use is call-only, scoped, retained, or ownership-transferring. A matching typed
context operation reconstructs the checked borrow without synthesizing hidden captures.

Generated entry adapters preserve provider lifetime and establish or reuse the exact native-thread attachment. They
enter a synchronous root with the foreign boundary's declared execution requirements, contain panic before returning to
foreign code, and resolve lifecycle state before detachment. Nested entry reuses attachment while preserving distinct
invocation context. Concurrent use and reentry require their own wrapper contracts.

Retained contexts require deregistration and in-flight quiescence before destruction. A trampoline cannot recover that
lifetime by dereferencing already retired storage. Panic handling uses the declared representable result or termination
policy, never foreign unwinding or a fabricated default return.

## Target bindings

Exact target gates select low-level modules. Pinned SDK input manifests generate authoritative Bray constants and
declarations, with digests participating in ordinary source and artifact identity. Cross compilation never reads host
headers to fill missing target values. The generator owns these definitions, while target metadata validates them.

Trusted Bray owns conversions, resource state, loading policy, and error adaptation. The runtime owns callback roots,
attachment, cancellation, and panic containment. `bray-platform` provides safe host mechanisms, while isolated native
support normalizes only irreducible ABI details. The temporal provider remains a separate third-party boundary.

## Compilation and artifacts

Checking records ABI, layout, target, transfer, and capability dependencies. Lowering generates export adapters and
stable symbols from semantic identity. Package interfaces preserve public contracts, and implementation artifacts retain
the executable templates and native metadata consumers require.

Static imports contribute native link requirements. Dynamic lookup requires loader support without a link-time
requirement for the looked-up symbol. Demand-driven product formation selects compatible artifacts, and the linker
consumes that plan. It does not infer types or discover libraries while checking source.

Operational failures stay typed library values with useful numeric native codes. Invalid source or artifact contracts
use structured compiler diagnostics. Both preserve specific causes and the ownership needed to resolve failure.

## Related documents

- [Language ABI and raw-memory rules](../language/targets-layout-abi-and-raw-memory.md)
- [Standard library](standard-library.md)
- [I/O and platform services](io-and-platform-services.md)
- [Cleanup storage and providers](cleanup-storage-and-reports.md)
- [Foreign standard-library behavior](../language/targets-layout-abi-and-raw-memory/foreign-standard-library.md)
