# I/O and platform services

Portable I/O combines ordinary `std` owners and policies, private trusted Bray adapters, and exact target mechanisms.
The [language chapter](../language/io-and-platform-services.md) owns caller-visible behavior. Native support does not
turn library APIs into compiler intrinsics or take over portable lifecycle policy.

## Library responsibilities

| Module | Responsibility |
| --- | --- |
| `std.io` | Byte streams, buffering, standard streams, and common failures |
| `std.path` | Lossless target-native paths and explicit text conversion |
| `std.fs` | Filesystem resources and operations |
| `std.process` | Process context, child owners, and typed Bray protocols |
| `std.time` | Clocks, temporal values, and timer adapters |
| `std.random` | Entropy and explicitly seeded generators |

These modules reuse core data, formatting, and memory contracts. Pure values can remain available when a target lacks a
service. Service operations follow ordinary target-conditional checking.

Byte transfer and text interpretation stay separate. Paths and process text preserve native units rather than assuming
UTF-8. Process context is an immutable startup snapshot, while standard streams have separate process-root owners.
Resource-specific types retain lifecycle and release authority even when native representations happen to match.

## Policy and mechanisms

Trusted Bray implements ordinary operating-system services through exact-target `std.os` declarations. Public wrappers
own buffering, conversion, traversal ordering, command policy, typed process protocols, cancellation composition, and
error adaptation. The pinned temporal provider is a deliberate third-party exception for calendar and timezone work.

Standard streams, files, pipes, and captured output have distinct operation and retention boundaries. Concrete standard
streams own synchronization. Generic transfer, buffering, formatting, and native leaves do not independently add guards.
A test host explicitly selects captured streams, rather than ordinary products probing for capture at runtime.

Generated direct bindings handle representable target ABIs. Narrow shims normalize mechanisms such as macro-only APIs or
unusual callback conventions. They do not implement portable Bray policy or expose implementation-language objects.

## Private role contracts

Closed typed roles describe callable shape, ownership, retained borrows, completion, effects, and target requirements.
The shared native catalog owns exact signatures and semantic records. Compiler and provider mappings derive from that
inventory rather than maintaining a prose catalog of role numbers and layouts.

Build metadata explicitly associates private declarations with roles. Validation checks identity, signature, ABI,
semantic contract, and availability before safe wrappers rely on them. Public interfaces retain ordinary inferred
contracts, without private roles becoming declaration identities or source lookup names.

The ABI uses fixed-width values, validated buffers, resource-specific handles, and typed status categories. Providers
retain a borrowed range only when the declared operation lifetime permits it. Variable-length results use caller-owned
storage or bounded incremental transfer. Public strings, paths, and protocol values are constructed by Bray wrappers.

Numeric platform codes can accompany typed operational failures. Compiler diagnostics instead describe invalid source,
bindings, or artifacts through structured messages. Host prose does not replace either contract.

## Runtime integration

Platform mechanisms expose completion sources. Runtime roles own scheduling, waits, wakes, cancellation, and run
resolution. Private trusted owners connect the two and retain buffers and resources until terminal completion.
Cancellation requests alone do not end those dependencies.

Async adapters use event integration or a compatible blocking lane. Synchronous services can operate without selecting
an async scheduler. Callback entry requires an explicit root, panic, synchronization, and ownership contract rather than
calling arbitrary Bray code from a platform operation.

## Targets, artifacts, and demand

Typed target properties describe service availability. Artifact paths and provider selection remain explicit build
inputs. A provided service must satisfy its complete role contract, and an unavailable service does not trigger portable
emulation or host discovery.

Reachable checked bindings contribute role requirements. Product formation merges them deterministically, validates
exact providers, and supplies typed link inputs. Runtime overrides resolve before standard-library implementations.
Temporal support and unrelated service families have independently retainable artifacts, so a narrow product does not
pull in unused mechanisms or runtime infrastructure.

Role validation, imported contracts, target selection, and artifact reads follow ordinary demand-driven queries over
immutable inputs. Parallel work does not change requirement ordering or diagnostics. Linkers consume the completed plan
without selecting services from unresolved symbol names.

## Related documents

- [Standard library](standard-library.md)
- [Foreign interoperability](foreign-and-platform-interoperability.md)
- [Time library](time.md)
- [Async runtime](async-runtime.md)
- [Native role catalog](../../crates/bray-runtime-abi/src/catalog.rs)
