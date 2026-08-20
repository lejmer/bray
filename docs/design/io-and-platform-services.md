# I/O And Platform Services

This document defines the implementation architecture for Bray's public I/O and platform-facing standard-library modules
and the private target mechanism boundary beneath them. The language-level surface is specified in
[I/O and platform services](../language/io-and-platform-services.md).

The contract separates three concerns:

- ordinary public `std` declarations define portable values, owners, policies, and errors,
- private trusted Bray declarations adapt those declarations to closed platform-service roles,
- target artifacts provide the irreducible operating-system or product-host mechanisms for those roles.

The boundary must not turn source-visible standard-library APIs into compiler intrinsics, expose host implementation
details, or move portable ownership and policy into native support code.

## Goals

The design must provide:

- one coherent public package layout for byte streams, paths, files, processes, clocks, entropy, and random generation,
- explicit ownership and lifecycle contracts for every external resource,
- typed operational failures without host-authored user-facing prose,
- synchronous and asynchronous composition without blocking cooperative runtime lanes,
- exact target and ABI compatibility checks before linking,
- demand-driven selection of only the platform roles required by reachable code,
- deterministic compiler service contracts and diagnostics regardless of parallel scheduling,
- and a narrow mechanism ABI that can be implemented by direct system bindings, Bray artifacts, or native shims.

The design does not require one operating system, object format, runtime implementation language, or code-generation
backend.

## Public Package Layout

The public surface belongs to the `std` package:

| Module        | Responsibility                                                                                                |
|---------------|---------------------------------------------------------------------------------------------------------------|
| `std.io`      | Byte reader and writer contracts, standard streams, buffering, flushing, and common I/O failures              |
| `std.path`    | Lossless target-native path values, path composition, normalization, comparison, and explicit text conversion |
| `std.fs`      | Files, open policy, seeking, metadata, directories, traversal, and filesystem mutation                        |
| `std.process` | Process arguments and environment snapshots, raw child processes, and higher-level typed Bray child processes |
| `std.time`    | Durations, monotonic instants, wall-clock values, deadlines, and timer adapters                               |
| `std.random`  | System entropy and deterministic pseudorandom generators                                                      |

The declaration names, parameter modes, owner types, error types, blocking requirements, asynchronous variants, and
result shapes are defined by the [I/O and platform standard-library surface](io-and-platform-surface.md).
Standard-library source and package interfaces must preserve that declaration-level contract.

These are ordinary Bray modules. Their declarations follow normal visibility, import, overload, implementation,
ownership, checking, and compiled-interface rules. No module or declaration becomes ambient merely because it belongs to
`std`.

Pure value contracts can remain available when a target lacks the corresponding external service. Service operations are
target-conditional according to the selected target profile.

The modules share the core text, byte, collection, formatting, hashing, ordering, and numeric contracts rather than
defining local substitutes. `std.io` and `std.fs` operate on byte sequences and mutable byte storage. Text decoding and
encoding are explicit operations. `std.path` preserves target-native path identity and never assumes that every path is
valid UTF-8.

## Public Contract Rules

### Streams

Reader and writer contracts are byte-oriented. A read or write can transfer fewer bytes than requested. A successful
zero-byte read from a non-empty destination denotes end of stream. A successful zero-byte write from a non-empty source
is rejected as a no-progress failure by the safe wrapper.

Flush is explicit. Buffering never changes the ownership, failure, ordering, or visibility contract of the wrapped
stream. A buffered writer cannot report successful finalization until buffered bytes have either been committed or
returned through a typed failure path.

Standard input, output, and error are process-root resources. Public handles are nonforgeable standard-library values.
Multiple handles to a standard output stream share the same product-owned destination through standard-library
synchronization rather than through unsynchronized global mutation.

### Paths And Filesystems

Paths preserve the selected target's native path units losslessly. Conversion from text validates the target's path
restrictions. Conversion to UTF-8 text is fallible when a native path cannot be represented. Value equality and ordering
compare the exact native representation using a target-defined total ordering, not a lossy display string or a
filesystem lookup. Lexical normalization is an explicit operation that produces another path value and does not claim
that two paths identify the same filesystem object.

File, directory, traversal, and other external-resource handles are non-copyable owners. Successful construction
transfers exactly one ownership obligation to the returned value. Explicit close or consuming completion resolves that
obligation. Lifecycle behavior must preserve close, flush, and error contracts and cannot silently discard a fallible
finalization result on a normal scope exit.

Filesystem operations accept explicit paths. Relative paths resolve against the immutable process-start
working-directory snapshot. The standard library does not expose mutation of a process-wide current directory.
Child-process builders carry their own explicit working-directory selection.

Directory enumeration order is not inherited from the host. APIs promising deterministic traversal normalize entries by
the target path ordering before publication. APIs exposing incremental native enumeration must state that order is
unspecified and must not be used where reproducible ordering is required without an explicit normalization step.

### Process Context And Child Processes

Source arguments, environment entries, and the startup working directory form one immutable process-context snapshot.
Standard streams instead come from distinct process-root operations selected for the execution environment. The source
argument sequence excludes the executable path. Environment entries use lexicographic unsigned target-native
key-code-unit order. Public accessors observe the snapshot and do not repeatedly query mutable host-global state.

Environment values and arguments preserve target-native units losslessly and expose explicit fallible text conversion.
The public surface does not expose mutation of the current process environment. A child-process request constructs an
explicit environment from either an empty environment or the immutable startup snapshot, then applies caller-supplied
additions and removals.

A raw child-process owner remains responsible for the child until it has reached a terminal state and has been reaped.
Waiting, requesting termination, and reaping are distinct semantic operations. Requesting termination does not release
ownership. An operation that returns an infrastructure error must not lose a still-live or unreaped child.

The typed `std.process.Process<T>` protocol owner is a higher-level ordinary Bray abstraction over the raw child-process
contract. The raw owner handles arguments, environment, streams, termination, and reaping. The typed owner additionally
handles Bray protocol authentication, encoding, decoding, cancellation, panic reports, and `RunResult<T>` publication.
The private platform boundary does not implement those typed semantics.

### Clocks, Entropy, And Randomness

Monotonic time and wall-clock time are distinct types and operations. Monotonic instants are process-local ordering
values and cannot be serialized as wall-clock timestamps. Monotonic readings never move backward within one process.
Wall-clock readings can move in either direction when the host clock changes.

System entropy is an external I/O effect with typed failure. It never falls back to a clock, process identifier,
address, or another predictable value. Deterministic pseudorandom generators are ordinary Bray values with explicit seed
state. Requesting a generator seeded from system entropy is an explicitly nondeterministic, fallible operation.

Timer and sleep conveniences compose clock values with either a blocking wait or runtime event registration. They do not
create a second scheduler or hide a native thread per timer.

## Effects And Failure Values

All external interaction follows the language's I/O effect rules. Constant evaluation, predicate expressions, static
constraints, and other effect-free contexts cannot perform these operations.

Blocking operations require `blocking_execution()`. Public asynchronous adapters must suspend through runtime event
integration or perform work on a compatible blocking lane. They cannot block a cooperative worker while waiting for a
file, pipe, child process, or timer.

Expected operating-system and resource failures are ordinary typed `Result` values. Stable public failure categories
describe conditions such as unavailable service, permission denial, absence, invalid input, interruption, exhaustion,
broken stream, unsupported operation, timeout, and target-specific failure. A failure can retain an optional numeric
platform code for inspection, but host-authored prose is not a stable semantic field and is not used as a compiler
diagnostic message.

Panics remain reserved for violated Bray or trusted-boundary invariants. Capacity exhaustion, missing files, denied
access, unavailable entropy, child creation failure, and clock failure are not compiler panics.

## Private Platform Service ABI

The platform service ABI is a closed compiler-readable contract between trusted standard-library bindings and the
selected target support artifacts. It is not a Bray package, source namespace, public declaration family, or
symbol-spelling convention.

Each role has:

- one stable typed role identity,
- one private callable signature schema,
- ownership and borrowing behavior,
- blocking and suspension behavior,
- lifecycle and error behavior,
- required target properties and trusted capabilities,
- and synchronization, visibility, cancellation, and callback-root service contracts where relevant.

The minimum role families are:

| Role family      | Required mechanisms                                                                                                 |
|------------------|---------------------------------------------------------------------------------------------------------------------|
| Process context  | Import the immutable process identity, startup arguments, environment, and working directory                        |
| Standard streams | Read standard input and write, flush, and serialize standard output or standard error through distinct operations   |
| File streams     | Read, write, flush, seek, and close a typed file owner                                                              |
| Process pipes    | Read child output, write and flush child input, and close a typed pipe owner                                        |
| Filesystems      | Open files and directories, query metadata, enumerate entries, mutate filesystem state, and close handles           |
| Child processes  | Spawn with explicit arguments, environment, working directory, and stream policy. Wait, signal, terminate, and reap |
| Clocks           | Read process-local monotonic and wall clocks                                                                        |
| Temporal data    | Interpret calendar values and named timezone rules through the pinned native provider                               |
| Entropy          | Fill caller-owned mutable bytes from the target entropy source                                                      |
| Wait integration | Expose waitable completion sources that the runtime reactor can register and wake                                   |

The role set is intentionally mechanism-oriented. Path normalization, buffering, text conversion, directory sorting,
command policy, typed process protocols, random algorithms, cancellation policy, and public error composition remain
Bray code.

### Role Schema

Every role descriptor uses the following closed schema fields:

| Field            | Contract                                                                     |
|------------------|------------------------------------------------------------------------------|
| `role`           | Stable `u32` role identity from the catalog below                            |
| `parameters`     | Ordered parameter descriptors using the closed ABI shapes below              |
| `results`        | Ordered output descriptors committed according to the role's status contract |
| `call_mode`      | `nonblocking`, `may_block`, or `starts_operation`                            |
| `statuses`       | Exact allowed `PlatformStatusCategory` set                                   |
| `handle_effects` | Handle class, input ownership, and success, failure, or terminal transition  |
| `buffer_effects` | Call-only or operation-retained lifetime for every borrowed byte range       |
| `completion`     | Immediate completion or the required operation completion contract           |
| `cancellation`   | `not_applicable`, `request_only`, or `request_and_complete`                  |
| `capability`     | One closed trusted platform capability required to bind the role             |

The ABI shape vocabulary is:

| Shape                    | Binary contract                                                                         |
|--------------------------|-----------------------------------------------------------------------------------------|
| `u32`, `u64`, `i64`      | Fixed-width scalar in the selected target ABI                                           |
| `status`                 | `PlatformStatus` record                                                                 |
| `const_bytes(call)`      | Pointer plus `u64` length, readable only until the call returns                         |
| `mut_bytes(call)`        | Pointer plus `u64` length, writable only until the call returns                         |
| `const_bytes(operation)` | Pointer plus `u64` length retained read-only until operation completion                 |
| `mut_bytes(operation)`   | Pointer plus `u64` length retained writable until operation completion                  |
| `path`                   | Call-only bytes in the target-native path encoding named by the target contract         |
| `native_text`            | Call-only units in the target-native process-text encoding named by the target contract |
| `raw_address`            | One target-width non-owning address. Zero is invalid for a successful symbol lookup     |
| `span_list`              | Call-only pointer plus count of call-only byte spans                                    |
| `environment_list`       | Call-only pointer plus count of key and value byte-span pairs                           |
| `handle_ref<K>`          | Borrowed nonzero `u64` opaque handle of class `K`                                       |
| `handle_owner<K>`        | Owned nonzero `u64` opaque handle of class `K`                                          |
| `handle_owner<K>?`       | Optional owned handle result. Zero means absent and nonzero transfers ownership         |
| `out<T>`                 | Caller-owned aligned storage for one `T`, valid for the declared output statuses        |
| `child_request`          | Call-only `AbiChildRequest` record                                                      |
| `file_options`           | `AbiFileOptions` record                                                                 |
| `file_metadata`          | `AbiFileMetadata` record                                                                |
| `exit_status`            | `AbiExitStatus` record                                                                  |
| `start_result`           | `AbiStartResult` record                                                                 |
| `operation_result`       | `AbiOperationResult` record                                                             |

The defined descriptor encoding uses these closed ordinal tables:

| Descriptor field | Ordinals                                                                                                                                                                                                                                                                                                                                                                                                  |
|------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Direction        | `input = 0`, `output = 1`                                                                                                                                                                                                                                                                                                                                                                                 |
| Shape            | `u32 = 0`, `u64 = 1`, `i64 = 2`, `status = 3`, `bytes = 4`, `path = 5`, `span_list = 6`, `environment_list = 7`, `handle = 8`, `child_request = 9`, `file_options = 10`, `file_metadata = 11`, `exit_status = 12`, `start_result = 13`, `operation_result = 14`, `native_text = 15`, `date_time = 16`, `temporal_observation = 17`, `temporal_resolution = 18`, `temporal_value = 19`, `raw_address = 20` |
| Byte access      | `not_applicable = 0`, `immutable = 1`, `mutable = 2`                                                                                                                                                                                                                                                                                                                                                      |
| Lifetime         | `not_applicable = 0`, `call = 1`, `operation = 2`                                                                                                                                                                                                                                                                                                                                                         |
| Presence         | `required = 0`, `optional = 1`                                                                                                                                                                                                                                                                                                                                                                            |
| Handle class     | `not_applicable = 0`, `process_pipe = 1`, `file_stream = 2`, `directory = 3`, `child = 4`, `operation = 5`, `wait_source = 6`, `time_zone = 7`, `dynamic_library = 8`                                                                                                                                                                                                                                     |
| Handle state     | `not_applicable = 0`, `absent = 1`, `borrowed = 2`, `owned = 3`, `retained_borrow = 4`, `retained_owner = 5`, `consumed = 6`                                                                                                                                                                                                                                                                              |
| Call mode        | `nonblocking = 0`, `may_block = 1`, `starts_operation = 2`                                                                                                                                                                                                                                                                                                                                                |
| Completion       | `immediate = 0`, `status_only = 1`, `byte_transfer = 2`, `child_wait = 3`, `timer = 4`, `operation_dispatch = 5`                                                                                                                                                                                                                                                                                          |
| Cancellation     | `not_applicable = 0`, `request_only = 1`, `request_and_complete = 2`                                                                                                                                                                                                                                                                                                                                      |
| Capability       | `process_context = 0`, `streams = 1`, `filesystem = 2`, `child_processes = 3`, `clocks = 4`, `entropy = 5`, `wait_integration = 6`, `temporal = 7`, `dynamic_loading = 8`                                                                                                                                                                                                                                 |

`const_bytes` and `mut_bytes` both encode as `bytes`. Byte access and lifetime distinguish them. `handle_ref` and
`handle_owner` both encode as `handle`. Handle class, presence, and the initial handle state distinguish them. `out<T>`
encodes `T` with output direction. Every field not applicable to a descriptor is encoded with its `not_applicable`
ordinal.

Every handle requirement accepts only its exact class. Standard-stream roles carry no forgeable handle because their
process-root identity is part of the selected execution environment.

Reference-provider filesystem and process handles use disjoint numeric domains: the high bit is clear for files and
directories and set for child processes and their pipes. Exact entry points also validate the owner variant, so a raw
value from another resource family cannot accidentally identify a different live owner.

Pointers have the selected target's pointer width and alignment. Every scalar length and offset is `u64`. The provider
rejects a value that cannot fit the target address space. All reserved fields and bits are zero. The provider validates
pointer, length, alignment, overlap, handle class, and enum values before using an input.

### ABI Record Layout

ABI records use the field order below. Each field starts at the smallest offset satisfying its target ABI alignment, the
record's alignment is its greatest field alignment, trailing padding extends the record to that alignment, and every
padding byte is zero. This rule and the selected target's pointer width determine all target-dependent offsets without
using an implementation-language object layout. Scalar fields use the selected target's byte order.

| Record                | Ordered fields                                                                                                                                                                                                  |
|-----------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `AbiByteSpan`         | `address: pointer`, `length: u64`                                                                                                                                                                               |
| `AbiSpanList`         | `entries: pointer<AbiByteSpan>`, `count: u64`                                                                                                                                                                   |
| `AbiEnvironmentEntry` | `key: AbiByteSpan`, `value: AbiByteSpan`                                                                                                                                                                        |
| `AbiEnvironmentList`  | `entries: pointer<AbiEnvironmentEntry>`, `count: u64`                                                                                                                                                           |
| `PlatformStatus`      | `category: u32`, `reserved: u32`, `native_code: i64`                                                                                                                                                            |
| `AbiStartResult`      | `state: u32`, `reserved: u32`, `value: u64`, `operation: u64`                                                                                                                                                   |
| `AbiOperationResult`  | `state: u32`, `reserved: u32`, `status: PlatformStatus`, `value0: u64`, `value1: u64`                                                                                                                           |
| `AbiFileOptions`      | `access: u32`, `creation: u32`, `reserved: u64`                                                                                                                                                                 |
| `AbiFileMetadata`     | `kind: u32`, `present: u32`, `bytes: u64`, `modified_seconds: i64`, `modified_nanoseconds: u32`, `reserved: u32`                                                                                                |
| `AbiExitStatus`       | `tag: u32`, `reserved: u32`, `payload: i64`                                                                                                                                                                     |
| `AbiChildRequest`     | `executable: AbiByteSpan`, `working_directory: AbiByteSpan`, `arguments: AbiSpanList`, `environment: AbiEnvironmentList`, `standard_input: u32`, `standard_output: u32`, `standard_error: u32`, `reserved: u32` |

`pointer<T>` is an address in the selected target address space, not an opaque handle. A zero address is valid only when
the paired count or length is zero. Lists are contiguous arrays of the named record. All nested child-request spans and
lists have call-only lifetime.

`PlatformStatus` is a 16-byte record containing `category: u32`, `reserved: u32`, and `native_code: i64`. The closed
categories are:

| Value | Category             | Meaning                                                                      |
|------:|----------------------|------------------------------------------------------------------------------|
|   `0` | `Success`            | The operation satisfied its success contract                                 |
|   `1` | `Unsupported`        | The target service cannot perform this operation                             |
|   `2` | `PermissionDenied`   | The platform denied the requested authority                                  |
|   `3` | `NotFound`           | The named resource does not exist                                            |
|   `4` | `AlreadyExists`      | Exclusive creation found an existing resource                                |
|   `5` | `InvalidInput`       | An otherwise well-formed ABI value violates operation policy                 |
|   `6` | `Interrupted`        | The platform interrupted the operation before normal completion              |
|   `7` | `Exhausted`          | Capacity or another finite platform resource is unavailable                  |
|   `8` | `BrokenStream`       | The peer or backing stream cannot continue the transfer                      |
|   `9` | `TimedOut`           | The requested deadline expired                                               |
|  `10` | `InsufficientBuffer` | The supplied output buffer is too small and the required length is available |
|  `11` | `Cancelled`          | A pending operation reached terminal cancellation                            |
|  `12` | `Other`              | A target failure has no more specific stable category                        |

`native_code` is zero when the provider has no target code. It is inspection data and cannot redefine the stable
category.

`AbiStartResult` contains `state: u32`, `reserved: u32`, `value: u64`, and `operation: u64`. State `0` is `Completed`
and requires an invalid zero operation handle. State `1` is `Pending` and requires a newly owned nonzero operation
handle. `value` carries the completed byte count when the role transfers bytes and is zero for other roles.

`AbiOperationResult` contains `state: u32`, `reserved: u32`, `status: PlatformStatus`, `value0: u64`, and `value1: u64`.
State `0` is `Pending`. State `1` is `Terminal`. A terminal result consumes the operation handle. For stream operations
`value0` is the transferred byte count. For child wait it identifies the terminal exit representation. Timer completion
leaves both values zero. For `Pending`, `status` is `Success` and both values are zero. For `Terminal`, `status` is a
category allowed by the originating role's completion profile. The outer `platform.operation.complete` status describes
only whether the completion query itself was valid.

`AbiFileOptions` values map one-to-one to the public `FileAccess` and `FileCreation` variants in declaration order. In
`AbiFileMetadata.present`, bit zero states whether the modified fields are present and every other bit is zero.
File-kind values map one-to-one to the public `FileKind` variants in declaration order. `AbiExitStatus.tag` maps
one-to-one to the public `ExitStatus` variants in declaration order.

The trusted Bray wrapper materializes the child request's complete environment from `EnvironmentPolicy` and its edits
before the ABI call. `AbiChildRequest` therefore carries a complete environment rather than inheritance policy. Its
three stream-policy values map one-to-one to the public `ChildStreamPolicy` variants in declaration order. The provider
copies every value it needs after the call returns.

### Process Context Block

The platform provider owns one immutable product-lifetime block using context ABI version `1.0`. Its 72-byte header uses
little-endian integers at these byte offsets. Process-root standard streams are separate resources and are not encoded
in the block:

| Offset | Field                                             |
|-------:|---------------------------------------------------|
|    `0` | platform ABI major as `u16`                       |
|    `2` | platform ABI minor as `u16`                       |
|    `4` | native text unit width in bits as `u8`            |
|    `5` | environment key comparison kind as `u8`           |
|    `6` | zero reserved `u16`                               |
|    `8` | process identity as `u64`                         |
|   `16` | startup working-directory payload offset as `u64` |
|   `24` | startup working-directory byte length as `u64`    |
|   `32` | argument count as `u64`                           |
|   `40` | argument table offset as `u64`                    |
|   `48` | environment entry count as `u64`                  |
|   `56` | environment table offset as `u64`                 |
|   `64` | complete block byte length as `u64`               |

Each argument table entry is a 16-byte payload offset and length pair. Each environment entry is a 32-byte key offset,
key length, value offset, and value length tuple. Every range is within the complete block and ranges cannot overlap
either table. Arguments retain source order and environment entries use lexicographic unsigned target-native
key-code-unit order.

The platform host fixes and validates the block before returning its first borrowed context view. Typed context roles
return scalar values or product-lifetime borrowed native-text views into that block. A provider cannot use these roles
to expose later host-global mutations. The standard library copies a borrowed view only when constructing an owned
argument, environment entry, or path value. Process identity and context counts require no allocation or copying.

`platform.context.environment_key_equals` compares two call-only native-text values using the target process
environment's key comparison rules. It writes `1` for equality and `0` otherwise. This role does not query mutable host
environment state.

### Closed Role Catalog

All roles return `status`. Output storage is committed only for `Success`, except that byte-transfer counts are valid
for every transfer status and required lengths are valid for `InsufficientBuffer`.

The catalog uses these exact status sets. Each hexadecimal value is the `u64` mask whose bit is the corresponding
`PlatformStatusCategory` numeric value:

| Set                 |     Mask | Categories                                                                                                                     |
|---------------------|---------:|--------------------------------------------------------------------------------------------------------------------------------|
| `context`           | `0x1003` | `Success`, `Unsupported`, `Other`                                                                                              |
| `context_indexed`   | `0x100b` | `Success`, `Unsupported`, `NotFound`, `Other`                                                                                  |
| `stream`            | `0x1363` | `Success`, `Unsupported`, `InvalidInput`, `Interrupted`, `BrokenStream`, `TimedOut`, `Other`                                   |
| `filesystem`        | `0x10ff` | `Success`, `Unsupported`, `PermissionDenied`, `NotFound`, `AlreadyExists`, `InvalidInput`, `Interrupted`, `Exhausted`, `Other` |
| `filesystem_buffer` | `0x14ff` | `filesystem` plus `InsufficientBuffer`                                                                                         |
| `child`             | `0x13ff` | `filesystem` plus `BrokenStream` and `TimedOut`                                                                                |
| `clock`             | `0x1043` | `Success`, `Unsupported`, `Interrupted`, `Other`                                                                               |
| `entropy`           | `0x10c3` | `Success`, `Unsupported`, `Interrupted`, `Exhausted`, `Other`                                                                  |
| `operation_control` | `0x1021` | `Success`, `InvalidInput`, `Other`                                                                                             |
| `dynamic_loading`   | `0x10ef` | `Success`, `Unsupported`, `PermissionDenied`, `NotFound`, `InvalidInput`, `Interrupted`, `Exhausted`, `Other`                  |

#### Process context

|       ID | Role                                      | Parameters                              | Results                                       | Status set        | Mode and effects                                                     |
|---------:|-------------------------------------------|-----------------------------------------|-----------------------------------------------|-------------------|----------------------------------------------------------------------|
| `0x0001` | `platform.context.identity`               | none                                    | `u64 identity`                                | none              | `nonblocking`. Returns an infallible product-lifetime scalar         |
| `0x0002` | `platform.context.native_text_width`      | none                                    | `u32 width`                                   | none              | `nonblocking`. Returns an infallible product-lifetime scalar         |
| `0x0003` | `platform.context.working_directory`      | none                                    | `out<raw_address> address`, `out<u64> length` | `context`         | `nonblocking`. Returns a product-lifetime borrowed view              |
| `0x0004` | `platform.context.argument_count`         | none                                    | `out<u64> count`                              | `context`         | `nonblocking`. No ownership change                                   |
| `0x0005` | `platform.context.argument`               | `u64 index`                             | `out<raw_address> address`, `out<u64> length` | `context_indexed` | `nonblocking`. Returns a product-lifetime borrowed view              |
| `0x0006` | `platform.context.environment_count`      | none                                    | `out<u64> count`                              | `context`         | `nonblocking`. No ownership change                                   |
| `0x0007` | `platform.context.environment_entry`      | `u64 index`                             | two `out<raw_address, u64>` views             | `context_indexed` | `nonblocking`. Returns product-lifetime borrowed key and value views |
| `0x0008` | `platform.context.environment_key_equals` | `native_text left`, `native_text right` | `out<u32> equal`                              | `context`         | `nonblocking`. No ownership change                                   |

#### Resource-specific streams

|       ID | Role                              | Parameters                                                 | Results                | Status set | Mode and effects                                          |
|---------:|-----------------------------------|------------------------------------------------------------|------------------------|------------|-----------------------------------------------------------|
| `0x0101` | `platform.standard_input.read`    | `mut_bytes(call)`                                          | `out<u64> transferred` | `stream`   | `may_block`. Process-root input identity is implicit      |
| `0x0102` | `platform.standard_input.lock`    | none                                                       | none                   | `stream`   | `may_block`. Acquires product-wide input serialization    |
| `0x0103` | `platform.standard_input.unlock`  | none                                                       | none                   | `stream`   | `nonblocking`. Releases product-wide input serialization  |
| `0x0111` | `platform.standard_output.write`  | `const_bytes(call)`                                        | `out<u64> transferred` | `stream`   | `may_block`. Process-root output identity is implicit     |
| `0x0112` | `platform.standard_output.flush`  | none                                                       | none                   | `stream`   | `may_block`. Output identity is implicit                  |
| `0x0113` | `platform.standard_output.lock`   | none                                                       | none                   | `stream`   | `may_block`. Acquires product-wide output serialization   |
| `0x0114` | `platform.standard_output.unlock` | none                                                       | none                   | `stream`   | `nonblocking`. Releases product-wide output serialization |
| `0x0121` | `platform.standard_error.write`   | `const_bytes(call)`                                        | `out<u64> transferred` | `stream`   | `may_block`. Process-root error identity is implicit      |
| `0x0122` | `platform.standard_error.flush`   | none                                                       | none                   | `stream`   | `may_block`. Error identity is implicit                   |
| `0x0123` | `platform.standard_error.lock`    | none                                                       | none                   | `stream`   | `may_block`. Acquires product-wide error serialization    |
| `0x0124` | `platform.standard_error.unlock`  | none                                                       | none                   | `stream`   | `nonblocking`. Releases product-wide error serialization  |
| `0x0201` | `platform.file.read`              | `handle_ref<file_stream>`, `mut_bytes(call)`               | `out<u64> transferred` | `stream`   | `may_block`. File owner retained                          |
| `0x0202` | `platform.file.write`             | `handle_ref<file_stream>`, `const_bytes(call)`             | `out<u64> transferred` | `stream`   | `may_block`. File owner retained                          |
| `0x0203` | `platform.file.flush`             | `handle_ref<file_stream>`                                  | none                   | `stream`   | `may_block`. File owner retained                          |
| `0x0204` | `platform.file.seek`              | `handle_ref<file_stream>`, `u64 offset_bits`, `u32 origin` | `out<u64> position`    | `stream`   | `may_block`. File owner retained                          |
| `0x0205` | `platform.file.close`             | `handle_owner<file_stream>`                                | none                   | `stream`   | `may_block`. Consumes the file owner                      |
| `0x0301` | `platform.process_pipe.read`      | `handle_ref<process_pipe>`, `mut_bytes(call)`              | `out<u64> transferred` | `stream`   | `may_block`. Pipe owner retained                          |
| `0x0302` | `platform.process_pipe.write`     | `handle_ref<process_pipe>`, `const_bytes(call)`            | `out<u64> transferred` | `stream`   | `may_block`. Pipe owner retained                          |
| `0x0303` | `platform.process_pipe.flush`     | `handle_ref<process_pipe>`                                 | none                   | `stream`   | `may_block`. Pipe owner retained                          |
| `0x0304` | `platform.process_pipe.close`     | `handle_owner<process_pipe>`                               | none                   | `stream`   | `may_block`. Consumes the pipe owner                      |

Standard input, standard output, standard error, files, and process pipes have separate callable and artifact
boundaries. A value's resource kind is established when its public owner or process-root environment is constructed.
Ordinary calls never rediscover the kind by dispatching through a universal stream handle. Standard-stream locks are
likewise specific to input, output, or error.

Seek origin `0` is `SeekFrom.Start` and interprets `offset_bits` as an unsigned absolute offset. Origins `1` and `2` are
`SeekFrom.Current` and `SeekFrom.End` and interpret the same bits as a two's-complement `i64` offset. Other origins are
invalid.

Read and write starts use `byte_transfer`. Flush starts use `status_only`. Child-wait starts use `child_wait`. Timer
starts use `timer`. And `platform.operation.complete` uses `operation_dispatch`. Every other role uses `immediate`.
Start roles use `request_and_complete`, `platform.operation.cancel` uses `request_only`, and every other role uses
`not_applicable`.

#### Filesystems

|       ID | Role                             | Parameters                                 | Results                                                              | Status set          | Mode and effects                                     |
|---------:|----------------------------------|--------------------------------------------|----------------------------------------------------------------------|---------------------|------------------------------------------------------|
| `0x0211` | `platform.file.open`             | `path`, `file_options`                     | `out<handle_owner<file_stream>>`                                     | `filesystem`        | `may_block`. Creates owner only on success           |
| `0x0212` | `platform.file.metadata`         | `handle_ref<file_stream>`                  | `out<file_metadata>`                                                 | `filesystem`        | `may_block`. Handle retained                         |
| `0x0213` | `platform.path.metadata`         | `path`                                     | `out<file_metadata>`                                                 | `filesystem`        | `may_block`. No handle transition                    |
| `0x0221` | `platform.directory.open`        | `path`                                     | `out<handle_owner<directory>>`                                       | `filesystem`        | `may_block`. Creates owner only on success           |
| `0x0222` | `platform.directory.next`        | `handle_ref<directory>`, `mut_bytes(call)` | `out<u64> written_or_required`, `out<u32> end`, `out<file_metadata>` | `filesystem_buffer` | `may_block`. Advances only on success                |
| `0x0223` | `platform.directory.close`       | `handle_owner<directory>`                  | none                                                                 | `filesystem`        | `may_block`. Consumes owner on every terminal status |
| `0x0230` | `platform.path.create_directory` | `path`                                     | none                                                                 | `filesystem`        | `may_block`. No handle transition                    |
| `0x0231` | `platform.path.remove_file`      | `path`                                     | none                                                                 | `filesystem`        | `may_block`. No handle transition                    |
| `0x0232` | `platform.path.remove_directory` | `path`                                     | none                                                                 | `filesystem`        | `may_block`. No handle transition                    |
| `0x0233` | `platform.path.rename`           | `path source`, `path destination`          | none                                                                 | `filesystem`        | `may_block`. No handle transition                    |

`platform.directory.next` sets `end` to one only for `Success` with no entry. `InsufficientBuffer` reports the required
native path byte length and does not advance. Public asynchronous filesystem operations deliberately dispatch these same
roles to a checked blocking lane. The platform ABI has no second native asynchronous filesystem role family because the
public completion, cancellation, borrowing, and scheduling contracts are already expressed by the runtime lane and
common operation contracts.

#### Child processes

|       ID | Role                       | Parameters                      | Results                                                                         | Status set | Mode and effects                                    |
|---------:|----------------------------|---------------------------------|---------------------------------------------------------------------------------|------------|-----------------------------------------------------|
| `0x0311` | `platform.child.spawn`     | `child_request`                 | `out<handle_owner<child>>`, three `out<handle_owner<process_pipe>?>` pipe slots | `child`    | `may_block`. Outputs exist only on success          |
| `0x0312` | `platform.child.wait`      | `handle_ref<child>`             | `out<u32> terminal`, `out<exit_status>`                                         | `child`    | `may_block`. Child owner retained                   |
| `0x0313` | `platform.child.terminate` | `handle_ref<child>`, `u32 mode` | none                                                                            | `child`    | `may_block`. Requests termination and retains owner |
| `0x0314` | `platform.child.reap`      | `handle_owner<child>`           | `out<exit_status>`                                                              | `child`    | `may_block`. Consumes owner only on success         |
| `0x0315` | `platform.child.dispose`   | `handle_owner<child>`           | none                                                                            | `child`    | `may_block`. Resolves and consumes the owner        |

An unused pipe slot is zero. A successful spawn transfers every nonzero pipe owner to the trusted wrapper. A failed
spawn commits no handle. `platform.child.wait` never reaps. `platform.child.terminate` never claims terminal completion.
Reap is valid only after a terminal wait result and retains the child owner on failure so the wrapper can continue
cleanup.

Termination mode `0` requests the target's cooperative termination mechanism. Mode `1` requests forced termination. A
target that cannot perform the requested mode returns `Unsupported`. Every other value is invalid.

The public consuming completion operations do not return until reap succeeds. They can preserve the first infrastructure
failure while retrying ownership resolution, then return that failure after the child is reaped. The typed Bray process
wrapper applies its separate `TerminationPolicy` through these roles.

#### Clocks and entropy

|       ID | Role                           | Parameters                       | Results                                    | Status set | Mode and effects                                       |
|---------:|--------------------------------|----------------------------------|--------------------------------------------|------------|--------------------------------------------------------|
| `0x0401` | `platform.clock.monotonic_now` | none                             | `out<u64>` nanosecond ticks                | `clock`    | `nonblocking`. Output exists only on success           |
| `0x0402` | `platform.clock.wall_now`      | none                             | `out<i64> seconds`, `out<u32> nanoseconds` | `clock`    | `nonblocking`. No ownership change                     |
| `0x0403` | `platform.clock.sleep`         | `u64 seconds`, `u32 nanoseconds` | none                                       | `clock`    | `may_block`. No ownership change                       |
| `0x0411` | `platform.timer.start`         | `u64` nanosecond ticks           | `out<start_result>`                        | `clock`    | `starts_operation`. Terminal completion has no payload |
| `0x0501` | `platform.entropy.fill`        | `mut_bytes(call)`                | `out<u64> transferred`                     | `entropy`  | `may_block`. Initializes exactly the reported prefix   |

`platform.clock.monotonic_now` readings use the process-local monotonic clock domain, count nanoseconds, and never
decrease. `wall_now` nanoseconds are below one billion. Public async entropy dispatches the blocking role to a checked
blocking lane. Timer
cancellation uses the common operation roles and forwards cancellation only after terminal operation completion.

Each native provider converts its target monotonic source to nanoseconds at the platform boundary. The conversion scale
and process clock domain are properties of the selected provider rather than fields repeated in every observation.

Each process-context, stream, filesystem, child-process, clock, and entropy role requires its same-named capability. The
three operation roles require `wait_integration`. A target contract must advertise the capability and every role in that
capability's authoritative catalog before a trusted binding can use it.

`platform.timer.start` always returns a pending operation. Its terminal `AbiOperationResult` leaves both values zero.

#### Civil time and time zones

|       ID | Role                          | Parameters                                                                  | Results                                                                         | Mode and effects                               |
|---------:|-------------------------------|-----------------------------------------------------------------------------|---------------------------------------------------------------------------------|------------------------------------------------|
| `0x0701` | `platform.time.date_validate` | `i32 year`, `u32 month`, `u32 day`                                          | `out<u32> outcome`                                                              | `nonblocking`. No ownership change             |
| `0x0702` | `platform.time.date_add`      | `date_time`, `i32 years`, `i32 months`, `i32 days`, `u32 adjustment`        | `out<date_time>`, `out<u32> outcome`                                            | `nonblocking`. No ownership change             |
| `0x0710` | `platform.time.zone_load`     | `native_text(call)`                                                         | `out<handle_owner<time_zone>>`, `out<u32> outcome`                              | `may_block`. Creates an owner on success       |
| `0x0711` | `platform.time.zone_local`    | none                                                                        | `out<handle_owner<time_zone>>`, `out<u32> outcome`                              | `may_block`. Creates an owner on success       |
| `0x0712` | `platform.time.zone_retain`   | `handle_ref<time_zone>`                                                     | none                                                                            | `nonblocking`. Creates one additional owner    |
| `0x0713` | `platform.time.zone_close`    | `handle_owner<time_zone>`                                                   | none                                                                            | `nonblocking`. Consumes one owner              |
| `0x0714` | `platform.time.zone_name`     | `handle_ref<time_zone>`, `mut_bytes(call)`                                  | `out<u64> written_or_required`, `out<u32> outcome`                              | `nonblocking`. Initializes the reported prefix |
| `0x0720` | `platform.time.observe`       | `handle_ref<time_zone> or zero`, fixed offset, timestamp, `mut_bytes(call)` | `out<temporal_observation>`, `out<u64> written_or_required`, `out<u32> outcome` | `nonblocking`. Initializes the reported prefix |
| `0x0721` | `platform.time.resolve`       | `handle_ref<time_zone> or zero`, fixed offset, `date_time`                  | `out<temporal_resolution>`, `out<u32> outcome`                                  | `nonblocking`. No ownership change             |
| `0x0730` | `platform.time.parse`         | `u32 kind`, `native_text(call)`                                             | `out<temporal_value>`, `out<u64> invalid_offset`, `out<u32> outcome`            | `nonblocking`. No ownership change             |
| `0x0731` | `platform.time.format`        | `u32 kind`, `temporal_value`, `mut_bytes(call)`                             | `out<u64> written_or_required`, `out<u32> outcome`                              | `nonblocking`. Initializes the reported prefix |

The `0x07xx` role family is backed by the static temporal provider described in [Time library](time.md). The roles
exchange fixed-width calendar fields, timestamps, caller-owned text buffers, and opaque process-local timezone
identities. They do not expose C++ layouts or depend on a host-installed timezone database. Exact UTC and fixed-offset
operations remain available without loading named-zone data.

#### Dynamic libraries

|       ID | Role                                   | Parameters                                              | Results                              | Status set        | Mode and effects                                                |
|---------:|----------------------------------------|---------------------------------------------------------|--------------------------------------|-------------------|-----------------------------------------------------------------|
| `0x0801` | `platform.dynamic_library.open_path`   | `path`, `u32 policy`                                    | `out<handle_owner<dynamic_library>>` | `dynamic_loading` | `may_block`, creates an owner only on success                   |
| `0x0802` | `platform.dynamic_library.open_system` | `u32 identity`, `u32 policy`                            | `out<handle_owner<dynamic_library>>` | `dynamic_loading` | `may_block`, creates an owner only on success                   |
| `0x0803` | `platform.dynamic_library.symbol`      | `handle_ref<dynamic_library>`, `const_bytes(call) name` | `out<raw_address>`                   | `dynamic_loading` | `may_block`, retains the library owner and creates no ownership |
| `0x0804` | `platform.dynamic_library.close`       | `handle_owner<dynamic_library>`                         | none                                 | `dynamic_loading` | `may_block`, consumes the owner on every terminal status        |

All four roles use immediate completion and `not_applicable` cancellation. Policy values are `0` for local visibility
with immediate resolution, `1` for local visibility with lazy resolution, `2` for global visibility with immediate
resolution, and `3` for global visibility with lazy resolution. A target returns `Unsupported` before opening when it
cannot provide the selected policy. Other values are `InvalidInput`.

`open_path` uses the target-native path exactly and performs no ambient search. `open_system` accepts only an identity
allocated by the selected target-specific standard-library module. It invokes the target's system-library facility
without consulting the working directory, process environment, package graph, or network. Unknown identities are
`InvalidInput`.

`symbol` requires a nonempty exact byte name without an interior NUL. `NotFound` means unavailable symbol for this role
and unavailable library for either open role. On success, `raw_address` is nonzero and remains valid only while the
borrowed library owner remains live. The role proves address existence only. The trusted Bray wrapper remains
responsible for the requested callable or data type. A failed close still consumes the native owner, matching the
best-effort destruction contract and preventing a second close of an indeterminate loader state.

The `dynamic_loading` capability requires all four roles and makes `target.platform.dynamic_loading` true. A target
without the complete catalog exposes none of the operations and sets the service contract to false.

Cancellation is a request, not a terminal result. Once requested, the provider eventually makes `operation.complete`
terminal. Normal completion wins a race that became terminal before cancellation was accepted. Otherwise accepted
cancellation completes with `Cancelled`. No cancellation path releases an operation owner, retained handle borrow, or
retained buffer before that terminal completion. Completion publishes retained buffer writes before releasing those
borrows.

### Contract Encoding And Hashing

The compiler and provider encode role descriptors in ascending numeric role order. The defined little-endian encoding
contains:

1. platform ABI major and minor as `u16`,
2. descriptor count as `u32`,
3. for each role, its `u32` identity,
4. parameter, result, handle-transition, and buffer-effect counts as `u16`,
5. each ordered parameter and result descriptor as shape, direction, byte access, lifetime, presence, handle class, and
   initial handle-state `u8` values,
6. call mode, completion profile, cancellation mode, and capability as `u8`,
7. allowed status-category bits as `u64`,
8. each ordered handle transition as subject kind `u8`, subject index `u16`, and initial, success, failure, pending, and
   terminal handle-state `u8` values,
9. and each ordered buffer effect as parameter index `u16`, byte-access `u8`, and lifetime `u8`.

Handle-transition subject kind `0` means parameter and `1` means result. Counts and indices are encoded little-endian.
Descriptor fields are emitted in callable order. Handle transitions and buffer effects are emitted by subject kind and
then ascending index. The callable's direct `status` return is implicit and is not included in the result count.

Shape, direction, byte-access, lifetime, presence, handle-class, handle-state, call-mode, completion, cancellation-mode,
capability, and transition values use the closed ordinal tables defined by this section. The descriptor contains no role
names, symbol spellings, source paths, artifact paths, or host prose.

The semantic-contract digest is:

```text
BLAKE3(
    "bray.platform.role-contract\0"
    || little_endian_u64(stable_descriptor_bytes.length)
    || stable_descriptor_bytes
)
```

The numeric role identity binds the language-defined semantics in this catalog. Changing a role's observable semantics,
ABI shape, ownership transitions, buffer lifetime, completion behavior, or status interpretation without a compatible
platform ABI change is nonconforming. Binding validation compares the complete descriptor and digest before accepting a
private declaration.

The descriptor records handle effects as ordered transition tuples. A borrowed input remains `borrowed` after immediate
completion. A start role changes each operation-retained input to `retained_borrow` only when it returns `Pending`, then
restores `borrowed` at terminal completion. A successful required owner result changes from `absent` to `owned`. It
remains `absent` on failure. A successful optional owner result changes to `owned` when nonzero and remains `absent`
when zero. A close role changes its input from `owned` to `consumed` on every terminal status. `operation.complete`
changes its operation input to `retained_owner` while pending and `consumed` when terminal. The wait-source result is
`borrowed` and becomes invalid with terminal operation completion. These rules cover every handle effect in the
authoritative catalog and are part of the defined descriptor.

### ABI Values

The private ABI uses fixed-width scalars, validated pointer-and-length borrows, opaque handles, and closed status
values. It does not exchange Rust, C++, or implementation-language object layouts. A provider cannot retain a borrowed
buffer after an operation returns unless the role contract explicitly transfers that borrow into a registered
asynchronous operation whose lifetime is tied to a checked frame or owner.

Caller-owned buffers cross the boundary by borrow. Providers do not allocate public Bray strings, collections, paths,
errors, or protocol values. Variable-length results use a size query, caller-supplied storage, or a bounded incremental
operation. Numeric platform codes remain uninterpreted data until a trusted Bray adapter maps them into stable public
failure categories.

Opaque handles are target-local and process-local. They are not pointers in the public surface, cannot be forged by
source, cannot be serialized into compiled package interfaces, and cannot cross an independent process except through a
separate declared transfer contract. Every owned handle role identifies the operation that consumes or closes it.

### Binding And Validation

Private trusted standard-library declarations are explicitly associated with platform roles in product build metadata.
The compiler validates the role identity, callable shape, target availability, platform-service ABI compatibility, and
semantic contract before checking the binding and its safe wrappers. It never infers a role from a source path,
declaration name, extern symbol, or body implementation language.

Role associations stay private. Public `std` package interfaces contain only the ordinary inferred contracts of public
wrappers. They do not expose private role identities, symbols, handles, or native error codes as declaration identity.

Platform-service ABI compatibility uses one typed major and minor version for the complete closed contract. Consumers
require an equal major version and a provider minor version no older than the required minor version. Individual role
families do not invent independent version systems. A target artifact records the exact ABI version and roles it
provides.

## Runtime Integration

The platform service ABI and protected-frame runtime ABI have separate ownership:

- platform roles provide external mechanisms and waitable completion sources,
- runtime roles schedule frames, register waits, wake tasks, propagate cancellation, and resolve run ownership.

An async standard-library wrapper joins the two through a private trusted Bray owner. Registration transfers the wait
obligation to that owner. Completion or cancellation removes the registration exactly once, resolves any retained buffer
or handle dependency, and wakes the suspended frame. A synchronous-only product can use blocking stream, file, clock,
entropy, and child-process roles without linking the async runtime.

No platform operation may call arbitrary Bray source without a role contract that establishes a valid callback execution
root, panic boundary, ownership transfer, and synchronization edge.

## Target Properties And Availability

The selected target profile exposes these boolean service contracts under `target.platform`:

```text
target.platform.process_context
target.platform.standard_streams
target.platform.filesystem
target.platform.child_processes
target.platform.monotonic_clock
target.platform.wall_clock
target.platform.entropy
target.platform.time_zones
```

These service contracts describe language-level availability. A true service contract requires the selected target
support artifacts to provide every mandatory role for that service. A false service contract makes the corresponding
service operations unavailable during normal target-conditional declaration checking.

Target properties do not select a provider or encode an artifact path. Exact provider identity, role bindings, ABI
version, native dependencies, and artifact digests remain build and link inputs.

## Artifacts And Linking

Each standard-library target artifact set records:

- the exact target identity,
- the platform-service ABI version,
- provided platform roles and their semantic-contract digest,
- the implementing static archive, direct system binding, or narrow native shim for each role,
- native dependency requirements,
- and content digests for every supplied artifact.

The reference native provider is packaged independently of the protected-frame concurrency runtime. Standard-library
target inventories carry capability-partitioned archives, and each archive carries only its own direct system-library
requirements. A runtime component may declare the exact platform roles it overrides, such as a test host's reserved
standard input and bounded standard-output and standard-error capture leaves. The ordinary provider archives remain
available for every other reachable role. This separation ensures that a synchronous product can use native platform
services without acquiring task scheduling, protected-frame storage, or other concurrency-runtime code.

Platform-provider archives have distinct typed link provenance and follow the selected runtime components in archive
resolution order. Runtime-owned exact operations therefore override their ordinary provider leaves deterministically,
while unresolved operations continue into the capability-partitioned provider archives.

Direct platform bindings are preferred when the target exposes a stable representable ABI. A native shim is allowed only
to normalize mechanisms that cannot be expressed safely through direct declarations, such as macro-only APIs, unstable
native structures, unusual calling conventions, or signal and unwind trampolines. A shim cannot own portable Bray
policy.

Standard input, standard output, standard error, files, process pipes, sockets, and captured test streams occupy
independent retention boundaries. The production standard-stream leaves call only the exact target stream mechanism and
do not route through Rust `std::io`, capture state, filesystem dispatch, or process-pipe dispatch. Test-runner products
select a capture-capable host component explicitly. Ordinary products never probe for capture at runtime.

Compilation gathers required roles from reachable checked standard-library bindings. Product formation merges those
requirements deterministically, validates the selected provider, and publishes typed link-plan inputs.
Capability-specific archives and object leaves ensure that unused service families do not contribute implementation
members or the async runtime to the product. The linker consumes the validated plan and does not reinterpret one
universal stream operation at runtime.

## Demand-Driven Compiler Queries

Platform service contracts follow the compiler's ordinary lazy architecture:

1. Binding a public declaration requests its imported `std` declaration service contract.
2. Consumer checking requests the public contract and target availability without exposing private role identities.
3. Building trusted standard-library source requests private role contracts only for bindings whose bodies are checked.
4. Lowering publishes the required operation or external call without choosing an artifact by filesystem search.
5. Product formation merges role requirements from reachable selected standard-library artifacts.
6. Code generation and link planning request the exact selected target artifact records.

Independent wrappers and role validations can run in parallel from immutable inputs. Required-role merging, diagnostics,
and link inputs use stable typed identities and stable ordering so scheduling cannot affect the result.

## Diagnostics

Compile-time failures include unavailable target services, missing private roles, incompatible role signatures,
incompatible ABI versions, invalid semantic-contract records, missing target artifacts, and unsatisfied blocking or
async execution requirements.

Compiler phases emit structured diagnostics with typed target, role, declaration, ABI-version, artifact, and source
arguments. `bray-messages` renders user-facing text. Platform error strings and native loader prose are never
substituted for compiler diagnostic messages.

Runtime I/O failures remain typed Bray values. Tooling can inspect their stable category and numeric platform code
without requiring localized compiler diagnostics.

## Conformance

The standard-library test suite must cover:

- public package identity and normal visibility behavior,
- lossless path, argument, and environment round trips,
- partial stream progress, EOF, flush, and no-progress handling,
- explicit file, directory, stream, and child-process ownership resolution,
- immutable process-context snapshots,
- child termination followed by mandatory reaping,
- monotonic clock ordering and wall-clock discontinuity handling,
- entropy failure without predictable fallback,
- deterministic seeded random streams,
- blocking-context rejection and nonblocking async integration,
- target-unavailable declarations,
- role signature, target, ABI, and semantic-contract mismatch,
- omission of unused service artifacts and async runtime linkage,
- required and forbidden linker-map provenance for standard-output-only and file-only products,
- explicit bounded test-output capture without production-path capture routing,
- deterministic requirement merging and diagnostics under parallel scheduling,
- and direct bindings and native shims producing equal public behavior.

Synthetic target providers must be sufficient for contract tests. Conformance tests must not depend on a machine-wide
installation, network service, locale-specific host message, or one operating system's handle representation.

## Non-Goals

This contract does not define:

- networking,
- terminal presentation policy,
- a shell or command language,
- automatic executable discovery through host search paths,
- ambient environment mutation,
- a package manager or dependency downloader,
- a universal event-loop implementation,
- serialization synthesis for typed child processes,
- or portable emulation of a service that the target profile marks unavailable.
