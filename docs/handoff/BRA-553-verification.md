# BRA-553 verification

The change is based on BRA-621 at `fb69346e31b225c39105d095e9f900c9cebad97c`. Independent findings are recorded in [the separate handoff](BRA-553-other-work.md).

## Authority and lifetime

The authority regressions compile the actual production declarations. The 29 host tests reject owners fabricated from ordinary fields, copied acquisition data, mutated witnesses, raw pointers without storage evidence, and callbacks without their entry contract. Tests exercise all six native targets. Native role assignment comes from explicit runtime bindings and validated signatures and contracts, independent of source spelling or visibility.

Resident host storage and synchronization use protected initialized storage. The acquisition lease and storage anchor have separate purposes and an explicit matching contract. Result dependencies preserve moved whole-owner evidence while retaining storage dependencies for borrowed addresses and fields. Conditional destructor evidence belongs to its actual receiver. Only verified unconditional total synchronous unit destruction has zero cleanup capacity.

Cleanup ownership is admitted at registration and transferred at terminal exit. The native slot clears before the callback. The closing attachment remains resident until cleanup returns, including when cleanup reports an incident. Rust receives its identity only for the callback's duration and does not acquire another epoch. A caught Rust panic restores that temporary view. The full runtime suite passes 143 tests, and the platform suite passes 18.

Host preparation catches failure and resets the formation transition before disposing a caught report. Shutdown restores retained storage when key destruction fails. The current synchronous preparation and key operations contain no cancellation checkpoint. This is not a guarantee about future callbacks added to those paths.

Publication and restoration require the whole storage owner's witness. The owner records successful key release, so shutdown derives restoration from owned state and cannot republish a released key. Acquiring the host lock preserves only the explicitly retained storage owner's contract. It does not preserve arbitrary caller witnesses. All preparation, ownership-transfer and rollback helpers have zero outgoing cleanup capacity.

The replaced Rust identity allocator, main-thread owner and thread-exit registry are removed. Rust retains mechanical native thread operations and scoped observations of Bray-owned identities. The change adds roughly 3,300 production source lines and 3,300 test lines, counted separately from generated declarations. The production growth covers the Bray lifecycle and the shared compiler mechanisms needed to enforce its contracts.

## Checks

- Full standard-library source verification passes on all six targets, followed by bundle conformance.
- Native Windows execution covers allocation denial, first-exit allocation denial, key creation and destruction failure, recursive entry, nested identity reuse, eight concurrent attachments, shutdown rejection while attached, native thread exit, and exact callback identity after TLS clearing.
- Generated native status adapters execute cancellation without reporting a panic and emit success, panic and cancellation paths on all six targets.
- Checker, codegen, IR, lowering, package-interface, platform and runtime-ABI suites pass. The broader compilation suite exposed standard-library fixture contracts, which were corrected and rerun. Its independent compiler-owned cleanup-role failure is documented in the handoff.
- The native link map places every migrated identity, attachment and thread-exit symbol in the Bray bootstrap archive. The replaced Rust exports are removed.

Detailed logs, native probes and the behavior audit are retained under the primary checkout's `target/bra-553/`. Shared Cargo and LLVM storage must not be cleaned as task-owned output.

## Cost

Initial attachment performs one explicit allocation of 96 bytes and creates one target TLS key on Windows x64. This counts the runtime's allocator calls, not allocations internal to the operating system. One thousand ordinary identity lookups perform no allocation. An uncontended lookup obtains the native thread ID, takes the host lock with one compare-exchange, reads the published key and target TLS slot, then releases the lock. Main-thread identity is one atomic load. The host adds no scheduler or optional-service registry.

A minimal initialization/shutdown C host was linked against freshly built BRA-621 and BRA-553 bootstrap archives with identical compiler and linker options. Both returned success. BRA-621 performed zero explicit startup allocations. The native section comparison, including the common C host shims, is:

| Section | BRA-621 bytes | BRA-553 bytes |
| --- | ---: | ---: |
| Code | 200,838 | 402,918 |
| Read-only data | 5,396 | 7,412 |
| Writable data | 440 | 512 |
| Unwind tables | 1,344 | 2,244 |
| Relocations | 44 | 44 |

The 72-byte writable-data increase holds the resident owner and its synchronization. The code increase includes generated lifecycle and native adapters for the new owner and attachment operations. These measurements cover the selected bundled bootstrap archive and its exported entries, without the Rust scheduler. They do not claim whole-program elimination of unused exported entries.
