# I/O And Platform Services

This document defines the implementation architecture for Bray's public I/O and platform-facing standard-library modules and the
private target mechanism boundary beneath them. The language-level surface is specified in
[I/O and platform services](../language/io-and-platform-services.md).

The contract separates three concerns:

- ordinary public `std` declarations define portable values, owners, policies, and errors,
- private trusted Bray declarations adapt those declarations to closed platform-service roles,
- target artifacts provide the irreducible operating-system or product-host mechanisms for those roles.

The boundary must not turn source-visible standard-library APIs into compiler intrinsics, expose host implementation details, or
move portable ownership and policy into native support code.

## Goals

The design must provide:

- one coherent public package layout for byte streams, paths, files, processes, clocks, entropy, and random generation,
- explicit ownership and lifecycle contracts for every external resource,
- typed operational failures without host-authored user-facing prose,
- synchronous and asynchronous composition without blocking cooperative runtime lanes,
- exact target and ABI compatibility checks before linking,
- demand-driven selection of only the platform roles required by reachable code,
- deterministic compiler facts and diagnostics regardless of parallel scheduling,
- and a narrow mechanism ABI that can be implemented by direct system bindings, Bray artifacts, or native shims.

The design does not require one operating system, object format, runtime implementation language, or code-generation backend.

## Public Package Layout

The public surface belongs to the canonical `std` package:

| Module | Responsibility |
| --- | --- |
| `std.io` | Byte reader and writer contracts, standard streams, buffering, flushing, and common I/O failures |
| `std.path` | Lossless target-native path values, path composition, normalization, comparison, and explicit text conversion |
| `std.fs` | Files, open policy, seeking, metadata, directories, traversal, and filesystem mutation |
| `std.process` | Process arguments and environment snapshots, raw child processes, and higher-level typed Bray child processes |
| `std.time` | Durations, monotonic instants, wall-clock values, deadlines, and timer adapters |
| `std.random` | System entropy and deterministic pseudorandom generators |

The canonical declaration names, parameter modes, owner types, error types, blocking requirements, asynchronous variants, and
result shapes are defined by the [I/O and platform standard-library surface](io-and-platform-surface.md). Standard-library source
and package interfaces must preserve that declaration-level contract.

These are ordinary Bray modules. Their declarations follow normal visibility, import, overload, implementation, ownership,
checking, and compiled-interface rules. No module or declaration becomes ambient merely because it belongs to `std`.

Pure value contracts can remain available when a target lacks the corresponding external service. Service operations are
target-conditional according to the selected target profile.

The modules share the core text, byte, collection, formatting, hashing, ordering, and numeric contracts rather than defining local
substitutes. `std.io` and `std.fs` operate on byte sequences and mutable byte storage. Text decoding and encoding are explicit
operations. `std.path` preserves target-native path identity and never assumes that every path is valid UTF-8.

## Public Contract Rules

### Streams

Reader and writer contracts are byte-oriented. A read or write can transfer fewer bytes than requested. A successful zero-byte read
from a non-empty destination denotes end of stream. A successful zero-byte write from a non-empty source is rejected as a
no-progress failure by the safe wrapper.

Flush is explicit. Buffering never changes the ownership, failure, ordering, or visibility contract of the wrapped stream. A
buffered writer cannot report successful finalization until buffered bytes have either been committed or returned through a typed
failure path.

Standard input, output, and error are process-root resources. Public handles are nonforgeable standard-library values. Multiple
handles to a standard output stream share the same product-owned destination through standard-library synchronization rather than
through unsynchronized global mutation.

### Paths And Filesystems

Paths preserve the selected target's native path units losslessly. Conversion from text validates the target's path restrictions.
Conversion to UTF-8 text is fallible when a native path cannot be represented. Value equality and ordering compare the exact native
representation using a target-defined total ordering, not a lossy display string or a filesystem lookup. Lexical normalization is
an explicit operation that produces another path value and does not claim that two paths identify the same filesystem object.

File, directory, traversal, and other external-resource handles are non-copyable owners. Successful construction transfers exactly
one ownership obligation to the returned value. Explicit close or consuming completion resolves that obligation. Lifecycle behavior
must preserve close, flush, and error contracts and cannot silently discard a fallible finalization result on a normal scope exit.

Filesystem operations accept explicit paths. Relative paths resolve against the immutable process-start working-directory snapshot.
The standard library does not expose mutation of a process-wide current directory. Child-process builders carry their own explicit
working-directory selection.

Directory enumeration order is not inherited from the host. APIs promising deterministic traversal normalize entries by the
target path ordering before publication. APIs exposing incremental native enumeration must state that order is unspecified and
must not be used where reproducible ordering is required without an explicit normalization step.

### Process Context And Child Processes

Arguments, environment entries, startup working directory, and standard-stream capabilities form one immutable process-context
snapshot. Public accessors observe that snapshot. They do not repeatedly query mutable host-global state.

Environment values and arguments preserve target-native units losslessly and expose explicit fallible text conversion. The public
surface does not expose mutation of the current process environment. A child-process request constructs an explicit environment
from either an empty environment or the immutable startup snapshot, then applies caller-supplied additions and removals.

A raw child-process owner remains responsible for the child until it has reached a terminal state and has been reaped. Waiting,
requesting termination, and reaping are distinct semantic operations. Requesting termination does not release ownership. An
operation that returns an infrastructure error must not lose a still-live or unreaped child.

The typed `std.process.Process<T>` protocol owner is a higher-level ordinary Bray abstraction over the raw child-process contract.
The raw owner handles arguments, environment, streams, termination, and reaping. The typed owner additionally handles Bray protocol
authentication, encoding, decoding, cancellation, panic reports, and `RunResult<T>` publication. The private platform boundary
does not implement those typed semantics.

### Clocks, Entropy, And Randomness

Monotonic time and wall-clock time are distinct types and operations. Monotonic instants are process-local ordering values and
cannot be serialized as wall-clock timestamps. Monotonic readings never move backward within one process. Wall-clock readings can
move in either direction when the host clock changes.

System entropy is an external I/O effect with typed failure. It never falls back to a clock, process identifier, address, or another
predictable value. Deterministic pseudorandom generators are ordinary Bray values with explicit seed state. Requesting a generator
seeded from system entropy is an explicitly nondeterministic, fallible operation.

Timer and sleep conveniences compose clock values with either a blocking wait or runtime event registration. They do not create a
second scheduler or hide a native thread per timer.

## Effects And Failure Values

All external interaction follows the language's I/O effect rules. Constant evaluation, predicate expressions, static constraints,
and other effect-free contexts cannot perform these operations.

Blocking operations require `blocking_execution()`. Public asynchronous adapters must suspend through runtime event integration or
perform work on a compatible blocking lane. They cannot block a cooperative worker while waiting for a file, pipe, child process,
or timer.

Expected operating-system and resource failures are ordinary typed `Result` values. Stable public failure categories describe
conditions such as unavailable service, permission denial, absence, invalid input, interruption, exhaustion, broken stream,
unsupported operation, timeout, and target-specific failure. A failure can retain an optional numeric platform code for inspection,
but host-authored prose is not a stable semantic field and is not used as a compiler diagnostic message.

Panics remain reserved for violated Bray or trusted-boundary invariants. Capacity exhaustion, missing files, denied access,
unavailable entropy, child creation failure, and clock failure are not compiler panics.

## Private Platform Service ABI

The platform service ABI is a closed compiler-readable contract between trusted standard-library bindings and the selected target
support artifacts. It is not a Bray package, source namespace, public declaration family, or symbol-spelling convention.

Each role has:

- one stable typed role identity,
- one private callable signature schema,
- ownership and borrowing behavior,
- blocking and suspension behavior,
- lifecycle and error behavior,
- required target facts and trusted capabilities,
- and synchronization, visibility, cancellation, and callback-root facts where relevant.

The minimum role families are:

| Role family | Required mechanisms |
| --- | --- |
| Process context | Import the immutable process identity, startup arguments, environment, working directory, and standard-stream handles |
| Streams | Read, write, flush, seek where supported, and close an opaque stream handle |
| Filesystems | Open files and directories, query metadata, enumerate entries, mutate filesystem state, and close handles |
| Child processes | Spawn with explicit arguments, environment, working directory, and stream policy; wait, signal, terminate, and reap |
| Clocks | Read monotonic and wall clocks and expose target resolution |
| Entropy | Fill caller-owned mutable bytes from the target entropy source |
| Wait integration | Expose waitable completion sources that the runtime reactor can register and wake |

The role set is intentionally mechanism-oriented. Path normalization, buffering, text conversion, directory sorting, command
policy, typed process protocols, random algorithms, cancellation policy, and public error composition remain Bray code.

### Canonical Role Schema

Every role descriptor uses the following closed schema fields:

| Field | Contract |
| --- | --- |
| `role` | Stable `u32` role identity from the catalog below |
| `parameters` | Ordered parameter descriptors using the closed ABI shapes below |
| `results` | Ordered output descriptors committed according to the role's status contract |
| `call_mode` | `nonblocking`, `may_block`, or `starts_operation` |
| `statuses` | Exact allowed `PlatformStatusCategory` set |
| `handle_effects` | Handle class, input ownership, and success, failure, or terminal transition |
| `buffer_effects` | Call-only or operation-retained lifetime for every borrowed byte range |
| `completion` | Immediate completion or the required operation completion contract |
| `cancellation` | `not_applicable`, `request_only`, or `request_and_complete` |
| `capability` | One closed trusted platform capability required to bind the role |

The ABI shape vocabulary is:

| Shape | Binary contract |
| --- | --- |
| `u32`, `u64`, `i64` | Fixed-width scalar in the selected target ABI |
| `status` | `PlatformStatus` record |
| `const_bytes(call)` | Pointer plus `u64` length, readable only until the call returns |
| `mut_bytes(call)` | Pointer plus `u64` length, writable only until the call returns |
| `const_bytes(operation)` | Pointer plus `u64` length retained read-only until operation completion |
| `mut_bytes(operation)` | Pointer plus `u64` length retained writable until operation completion |
| `path` | Call-only bytes in the target-native path encoding named by the target contract |
| `native_text` | Call-only units in the target-native process-text encoding named by the target contract |
| `span_list` | Call-only pointer plus count of call-only byte spans |
| `environment_list` | Call-only pointer plus count of key and value byte-span pairs |
| `handle_ref<K>` | Borrowed nonzero `u64` opaque handle of class `K` |
| `handle_owner<K>` | Owned nonzero `u64` opaque handle of class `K` |
| `handle_owner<K>?` | Optional owned handle result; zero means absent and nonzero transfers ownership |
| `out<T>` | Caller-owned aligned storage for one `T`, valid for the declared output statuses |
| `child_request` | Call-only `AbiChildRequest` record |
| `file_options` | `AbiFileOptions` record |
| `file_metadata` | `AbiFileMetadata` record |
| `exit_status` | `AbiExitStatus` record |
| `start_result` | `AbiStartResult` record |
| `operation_result` | `AbiOperationResult` record |

The canonical descriptor encoding uses these closed ordinal tables:

| Descriptor field | Ordinals |
| --- | --- |
| Direction | `input = 0`, `output = 1` |
| Shape | `u32 = 0`, `u64 = 1`, `i64 = 2`, `status = 3`, `bytes = 4`, `path = 5`, `span_list = 6`, `environment_list = 7`, `handle = 8`, `child_request = 9`, `file_options = 10`, `file_metadata = 11`, `exit_status = 12`, `start_result = 13`, `operation_result = 14`, `native_text = 15` |
| Byte access | `not_applicable = 0`, `immutable = 1`, `mutable = 2` |
| Lifetime | `not_applicable = 0`, `call = 1`, `operation = 2` |
| Presence | `required = 0`, `optional = 1` |
| Handle class | `not_applicable = 0`, `stream = 1`, `seekable_stream = 2`, `file_stream = 3`, `directory = 4`, `child = 5`, `operation = 6`, `wait_source = 7` |
| Handle state | `not_applicable = 0`, `absent = 1`, `borrowed = 2`, `owned = 3`, `retained_borrow = 4`, `retained_owner = 5`, `consumed = 6` |
| Call mode | `nonblocking = 0`, `may_block = 1`, `starts_operation = 2` |
| Completion | `immediate = 0`, `status_only = 1`, `byte_transfer = 2`, `child_wait = 3`, `timer = 4`, `operation_dispatch = 5` |
| Cancellation | `not_applicable = 0`, `request_only = 1`, `request_and_complete = 2` |
| Capability | `process_context = 0`, `streams = 1`, `filesystem = 2`, `child_processes = 3`, `clocks = 4`, `entropy = 5`, `wait_integration = 6` |

`const_bytes` and `mut_bytes` both encode as `bytes`; byte access and lifetime distinguish them. `handle_ref` and `handle_owner`
both encode as `handle`; handle class, presence, and the initial handle state distinguish them. `out<T>` encodes `T` with output
direction. Every field not applicable to a descriptor is encoded with its `not_applicable` ordinal.

A role requiring `stream` accepts `stream` or `file_stream`. A role requiring `seekable_stream` accepts `seekable_stream` or
`file_stream`. Every other handle requirement accepts only its exact class. This is the complete handle-class compatibility relation.

Pointers have the selected target's pointer width and alignment. Every scalar length and offset is `u64`; the provider rejects a
value that cannot fit the target address space. All reserved fields and bits are zero. The provider validates pointer, length,
alignment, overlap, handle class, and enum values before using an input.

### ABI Record Layout

ABI records use the field order below. Each field starts at the smallest offset satisfying its target ABI alignment, the record's
alignment is its greatest field alignment, trailing padding extends the record to that alignment, and every padding byte is zero.
This rule and the selected target's pointer width determine all target-dependent offsets without using an implementation-language
object layout. Scalar fields use the selected target's byte order.

| Record | Ordered fields |
| --- | --- |
| `AbiByteSpan` | `address: pointer`, `length: u64` |
| `AbiSpanList` | `entries: pointer<AbiByteSpan>`, `count: u64` |
| `AbiEnvironmentEntry` | `key: AbiByteSpan`, `value: AbiByteSpan` |
| `AbiEnvironmentList` | `entries: pointer<AbiEnvironmentEntry>`, `count: u64` |
| `PlatformStatus` | `category: u32`, `reserved: u32`, `native_code: i64` |
| `AbiStartResult` | `state: u32`, `reserved: u32`, `value: u64`, `operation: u64` |
| `AbiOperationResult` | `state: u32`, `reserved: u32`, `status: PlatformStatus`, `value0: u64`, `value1: u64` |
| `AbiFileOptions` | `access: u32`, `creation: u32`, `reserved: u64` |
| `AbiFileMetadata` | `kind: u32`, `present: u32`, `bytes: u64`, `modified_seconds: i64`, `modified_nanoseconds: u32`, `reserved: u32` |
| `AbiExitStatus` | `tag: u32`, `reserved: u32`, `payload: i64` |
| `AbiChildRequest` | `executable: AbiByteSpan`, `working_directory: AbiByteSpan`, `arguments: AbiSpanList`, `environment: AbiEnvironmentList`, `standard_input: u32`, `standard_output: u32`, `standard_error: u32`, `reserved: u32` |

`pointer<T>` is an address in the selected target address space, not an opaque handle. A zero address is valid only when the paired
count or length is zero. Lists are contiguous arrays of the named record. All nested child-request spans and lists have call-only
lifetime.

`PlatformStatus` is a 16-byte record containing `category: u32`, `reserved: u32`, and `native_code: i64`. The closed categories are:

| Value | Category | Meaning |
| ---: | --- | --- |
| `0` | `Success` | The operation satisfied its success contract |
| `1` | `Unsupported` | The target service cannot perform this operation |
| `2` | `PermissionDenied` | The platform denied the requested authority |
| `3` | `NotFound` | The named resource does not exist |
| `4` | `AlreadyExists` | Exclusive creation found an existing resource |
| `5` | `InvalidInput` | An otherwise well-formed ABI value violates operation policy |
| `6` | `Interrupted` | The platform interrupted the operation before normal completion |
| `7` | `Exhausted` | Capacity or another finite platform resource is unavailable |
| `8` | `BrokenStream` | The peer or backing stream cannot continue the transfer |
| `9` | `TimedOut` | The requested deadline expired |
| `10` | `InsufficientBuffer` | The supplied output buffer is too small and the required length is available |
| `11` | `Cancelled` | A pending operation reached terminal cancellation |
| `12` | `Other` | A target failure has no more specific stable category |

`native_code` is zero when the provider has no target code. It is inspection data and cannot redefine the stable category.

`AbiStartResult` contains `state: u32`, `reserved: u32`, `value: u64`, and `operation: u64`. State `0` is `Completed` and requires
an invalid zero operation handle. State `1` is `Pending` and requires a newly owned nonzero operation handle. `value` carries the
completed byte count when the role transfers bytes and is zero for other roles.

`AbiOperationResult` contains `state: u32`, `reserved: u32`, `status: PlatformStatus`, `value0: u64`, and `value1: u64`. State `0`
is `Pending`; state `1` is `Terminal`. A terminal result consumes the operation handle. For stream operations `value0` is the
transferred byte count. For child wait it identifies the terminal exit representation. Timer completion leaves both values zero.
For `Pending`, `status` is `Success` and both values are zero. For `Terminal`, `status` is a category allowed by the originating
role's completion profile. The outer `platform.operation.complete` status describes only whether the completion query itself was
valid.

`AbiFileOptions` values map one-to-one to the public `FileAccess` and `FileCreation` variants in declaration order. In
`AbiFileMetadata.present`, bit zero states whether the modified fields are present and every other bit is zero. File-kind values map
one-to-one to the public `FileKind` variants in declaration order. `AbiExitStatus.tag` maps one-to-one to the public `ExitStatus`
variants in declaration order.

The trusted Bray wrapper materializes the child request's complete environment from `EnvironmentPolicy` and its edits before the ABI
call. `AbiChildRequest` therefore carries a complete environment rather than inheritance policy. Its three stream-policy values map
one-to-one to the public `ChildStreamPolicy` variants in declaration order. The provider copies every value it needs after the call
returns.

### Process Context Block

The process-context roles expose one immutable product-lifetime block. Its 96-byte header uses little-endian integers at these byte
offsets:

| Offset | Field |
| ---: | --- |
| `0` | platform ABI major as `u16` |
| `2` | platform ABI minor as `u16` |
| `4` | native text unit width in bits as `u8` |
| `5` | environment key comparison kind as `u8` |
| `6` | zero reserved `u16` |
| `8` | process identity as `u64` |
| `16` | borrowed standard-input stream handle as `u64`, or zero when unavailable |
| `24` | borrowed standard-output stream handle as `u64`, or zero when unavailable |
| `32` | borrowed standard-error stream handle as `u64`, or zero when unavailable |
| `40` | startup working-directory payload offset as `u64` |
| `48` | startup working-directory byte length as `u64` |
| `56` | argument count as `u64` |
| `64` | argument table offset as `u64` |
| `72` | environment entry count as `u64` |
| `80` | environment table offset as `u64` |
| `88` | complete block byte length as `u64` |

Each argument table entry is a 16-byte payload offset and length pair. Each environment entry is a 32-byte key offset, key length,
value offset, and value length tuple. Every range is within the complete block and ranges cannot overlap either table. Entries and
payload retain product-start ordering; the public wrapper supplies target-aware lookup and deterministic ordered views.

The platform host fixes the block before the executable root starts. `platform.context.measure` and `platform.context.copy` observe
the same bytes for the product lifetime. A provider cannot use these roles to expose later host-global mutations.

`platform.context.environment_key_equals` compares two call-only native-text values using the target process environment's key
comparison rules. It writes `1` for equality and `0` otherwise. This role does not query mutable host environment state.

### Closed Initial Role Catalog

All roles return `status`. Output storage is committed only for `Success`, except that byte-transfer counts are valid for every
transfer status and required lengths are valid for `InsufficientBuffer`.

The catalog uses these exact status sets. Each hexadecimal value is the `u64` mask whose bit is the corresponding
`PlatformStatusCategory` numeric value:

| Set | Mask | Categories |
| --- | ---: | --- |
| `context` | `0x1003` | `Success`, `Unsupported`, `Other` |
| `context_buffer` | `0x1403` | `Success`, `Unsupported`, `InsufficientBuffer`, `Other` |
| `stream` | `0x1363` | `Success`, `Unsupported`, `InvalidInput`, `Interrupted`, `BrokenStream`, `TimedOut`, `Other` |
| `filesystem` | `0x10ff` | `Success`, `Unsupported`, `PermissionDenied`, `NotFound`, `AlreadyExists`, `InvalidInput`, `Interrupted`, `Exhausted`, `Other` |
| `filesystem_buffer` | `0x14ff` | `filesystem` plus `InsufficientBuffer` |
| `child` | `0x13ff` | `filesystem` plus `BrokenStream` and `TimedOut` |
| `clock` | `0x1043` | `Success`, `Unsupported`, `Interrupted`, `Other` |
| `entropy` | `0x10c3` | `Success`, `Unsupported`, `Interrupted`, `Exhausted`, `Other` |
| `operation_control` | `0x1021` | `Success`, `InvalidInput`, `Other` |

#### Process context

| ID | Role | Parameters | Results | Status set | Mode and effects |
| ---: | --- | --- | --- | --- | --- |
| `0x0001` | `platform.context.measure` | none | `out<u64> required` | `context` | `nonblocking`; no ownership change |
| `0x0002` | `platform.context.copy` | `mut_bytes(call) destination` | `out<u64> written_or_required` | `context_buffer` | `nonblocking`; `InsufficientBuffer` commits only required length |
| `0x0003` | `platform.context.environment_key_equals` | `native_text left`, `native_text right` | `out<u32> equal` | `context` | `nonblocking`; no ownership change |

#### Streams and asynchronous operations

| ID | Role | Parameters | Results | Status set | Mode and effects |
| ---: | --- | --- | --- | --- | --- |
| `0x0101` | `platform.stream.read` | `handle_ref<stream>`, `mut_bytes(call)` | `out<u64> transferred` | `stream` | `may_block`; handle retained |
| `0x0102` | `platform.stream.write` | `handle_ref<stream>`, `const_bytes(call)` | `out<u64> transferred` | `stream` | `may_block`; handle retained |
| `0x0103` | `platform.stream.flush` | `handle_ref<stream>` | none | `stream` | `may_block`; handle retained |
| `0x0104` | `platform.stream.seek` | `handle_ref<seekable_stream>`, `u64 offset_bits`, `u32 origin` | `out<u64> position` | `stream` | `may_block`; handle retained |
| `0x0105` | `platform.stream.close` | `handle_owner<stream>` | none | `stream` | `may_block`; consumes the owner on every terminal status |
| `0x0106` | `platform.stream.lock` | `handle_ref<stream>` | none | `stream` | `may_block`; acquires product-wide logical-operation serialization |
| `0x0107` | `platform.stream.unlock` | `handle_ref<stream>` | none | `stream` | `nonblocking`; releases product-wide logical-operation serialization |
| `0x0111` | `platform.stream.read.start` | `handle_ref<stream>`, `mut_bytes(operation)` | `out<start_result>` | `stream` | `starts_operation`; pending retains buffer and stream borrow |
| `0x0112` | `platform.stream.write.start` | `handle_ref<stream>`, `const_bytes(operation)` | `out<start_result>` | `stream` | `starts_operation`; pending retains buffer and stream borrow |
| `0x0113` | `platform.stream.flush.start` | `handle_ref<stream>` | `out<start_result>` | `stream` | `starts_operation`; pending retains stream borrow |
| `0x0601` | `platform.operation.wait_source` | `handle_ref<operation>` | `out<handle_ref<wait_source>>` | `operation_control` | `nonblocking`; returned borrow lives until operation completion |
| `0x0602` | `platform.operation.cancel` | `handle_ref<operation>` | none | `operation_control` | `nonblocking`; request only, operation and retained borrows remain |
| `0x0603` | `platform.operation.complete` | `handle_owner<operation>` | `out<operation_result>` | `operation_control` | `nonblocking`; pending retains owner, terminal consumes it and releases borrows |

Start roles return `Success` for both immediate and pending operation results. An immediate failure creates no operation and retains
no buffer. The outer status from `platform.operation.complete` reports whether completion polling itself was valid. A terminal
`AbiOperationResult.status` uses the status set of the originating role plus `Cancelled`.

Stream locks are shared by every standard-stream wrapper in one product. A wrapper acquires the lock before the first transfer of a
logical read, write-all, print, or flush operation and releases it after the operation's terminal success or failure. It does not
hold the lock across an asynchronous suspension. Providers serialize equal stream handles and allow unrelated handles to progress
independently.

Seek origin `0` is `SeekFrom.Start` and interprets `offset_bits` as an unsigned absolute offset. Origins `1` and `2` are
`SeekFrom.Current` and `SeekFrom.End` and interpret the same bits as a two's-complement `i64` offset. Other origins are invalid.

Read and write starts use `byte_transfer`; flush starts use `status_only`; child-wait starts use `child_wait`; timer starts use
`timer`; and `platform.operation.complete` uses `operation_dispatch`. Every other role uses `immediate`. Start roles use
`request_and_complete`, `platform.operation.cancel` uses `request_only`, and every other role uses `not_applicable`.

#### Filesystems

| ID | Role | Parameters | Results | Status set | Mode and effects |
| ---: | --- | --- | --- | --- | --- |
| `0x0201` | `platform.file.open` | `path`, `file_options` | `out<handle_owner<file_stream>>` | `filesystem` | `may_block`; creates owner only on success |
| `0x0202` | `platform.file.metadata` | `handle_ref<file_stream>` | `out<file_metadata>` | `filesystem` | `may_block`; handle retained |
| `0x0203` | `platform.path.metadata` | `path` | `out<file_metadata>` | `filesystem` | `may_block`; no handle transition |
| `0x0204` | `platform.directory.open` | `path` | `out<handle_owner<directory>>` | `filesystem` | `may_block`; creates owner only on success |
| `0x0205` | `platform.directory.next` | `handle_ref<directory>`, `mut_bytes(call)` | `out<u64> written_or_required`, `out<u32> end`, `out<file_metadata>` | `filesystem_buffer` | `may_block`; advances only on success |
| `0x0206` | `platform.directory.close` | `handle_owner<directory>` | none | `filesystem` | `may_block`; consumes owner on every terminal status |
| `0x0210` | `platform.path.create_directory` | `path` | none | `filesystem` | `may_block`; no handle transition |
| `0x0211` | `platform.path.remove_file` | `path` | none | `filesystem` | `may_block`; no handle transition |
| `0x0212` | `platform.path.remove_directory` | `path` | none | `filesystem` | `may_block`; no handle transition |
| `0x0213` | `platform.path.rename` | `path source`, `path destination` | none | `filesystem` | `may_block`; no handle transition |

`platform.directory.next` sets `end` to one only for `Success` with no entry. `InsufficientBuffer` reports the required native path
byte length and does not advance. Public asynchronous filesystem operations dispatch these same roles to a checked blocking lane;
the initial contract does not require a second native asynchronous filesystem role family.

#### Child processes

| ID | Role | Parameters | Results | Status set | Mode and effects |
| ---: | --- | --- | --- | --- | --- |
| `0x0301` | `platform.child.spawn` | `child_request` | `out<handle_owner<child>>`, three `out<handle_owner<stream>?>` pipe slots | `child` | `may_block`; outputs exist only on success |
| `0x0302` | `platform.child.wait` | `handle_ref<child>` | `out<u32> terminal`, `out<exit_status>` | `child` | `may_block`; child owner retained |
| `0x0303` | `platform.child.terminate` | `handle_ref<child>`, `u32 mode` | none | `child` | `may_block`; requests termination and retains owner |
| `0x0304` | `platform.child.reap` | `handle_owner<child>` | `out<exit_status>` | `child` | `may_block`; consumes owner only on success |
| `0x0311` | `platform.child.wait.start` | `handle_ref<child>` | `out<start_result>` | `child` | `starts_operation`; pending retains child borrow |

An unused pipe slot is zero. A successful spawn transfers every nonzero pipe owner to the trusted wrapper. A failed spawn commits no
handle. `platform.child.wait` never reaps. `platform.child.terminate` never claims terminal completion. Reap is valid only after a
terminal wait result and retains the child owner on failure so the wrapper can continue cleanup.

Termination mode `0` requests the target's cooperative termination mechanism. Mode `1` requests forced termination. A target that
cannot perform the requested mode returns `Unsupported`; every other value is invalid.

`platform.child.wait.start` always returns a pending operation. Its terminal `AbiOperationResult` encodes the exit-status tag in
`value0` and the signed payload's two's-complement bit pattern in `value1`.

The public consuming completion operations do not return until reap succeeds. They can preserve the first infrastructure failure
while retrying ownership resolution, then return that failure after the child is reaped. Raw async completion cancellation first
cancels and completes the pending wait operation, then requests forced termination and reaps under cancellation shielding before
forwarding current-run cancellation. The typed Bray process wrapper applies its separate `TerminationPolicy` through these roles.

#### Clocks and entropy

| ID | Role | Parameters | Results | Status set | Mode and effects |
| ---: | --- | --- | --- | --- | --- |
| `0x0401` | `platform.clock.monotonic_now` | none | `out<u64> ticks`, `out<u64> frequency`, `out<u64> clock_identity` | `clock` | `nonblocking`; no ownership change |
| `0x0402` | `platform.clock.wall_now` | none | `out<i64> seconds`, `out<u32> nanoseconds` | `clock` | `nonblocking`; no ownership change |
| `0x0403` | `platform.clock.sleep` | `u64 seconds`, `u32 nanoseconds` | none | `clock` | `may_block`; no ownership change |
| `0x0411` | `platform.timer.start` | `u64 ticks`, `u64 clock_identity` | `out<start_result>` | `clock` | `starts_operation`; terminal completion has no payload |
| `0x0501` | `platform.entropy.fill` | `mut_bytes(call)` | `out<u64> transferred` | `entropy` | `may_block`; initializes exactly the reported prefix |

`platform.clock.monotonic_now` readings with one clock identity use one stable frequency and never decrease. `wall_now` nanoseconds
are below one billion. Public async entropy dispatches the blocking role to a checked blocking lane. Timer cancellation uses the
common operation roles and forwards cancellation only after terminal operation completion.

Each process-context, stream, filesystem, child-process, clock, and entropy role requires its same-named capability. The three
operation roles require `wait_integration`. A target contract must advertise the capability and every role in that capability's
initial catalog before a trusted binding can use it.

`platform.timer.start` always returns a pending operation. Its terminal `AbiOperationResult` leaves both values zero.

Cancellation is a request, not a terminal result. Once requested, the provider eventually makes `operation.complete` terminal.
Normal completion wins a race that became terminal before cancellation was accepted; otherwise accepted cancellation completes with
`Cancelled`. No cancellation path releases an operation owner, retained handle borrow, or retained buffer before that terminal
completion. Completion publishes retained buffer writes before releasing those borrows.

### Contract Encoding And Hashing

The compiler and provider encode role descriptors in ascending numeric role order. The canonical little-endian encoding contains:

1. platform ABI major and minor as `u16`,
2. descriptor count as `u32`,
3. for each role, its `u32` identity,
4. parameter, result, handle-transition, and buffer-effect counts as `u16`,
5. each ordered parameter and result descriptor as shape, direction, byte access, lifetime, presence, handle class, and initial
   handle-state `u8` values,
6. call mode, completion profile, cancellation mode, and capability as `u8`,
7. allowed status-category bits as `u64`,
8. each ordered handle transition as subject kind `u8`, subject index `u16`, and initial, success, failure, pending, and terminal
   handle-state `u8` values,
9. and each ordered buffer effect as parameter index `u16`, byte-access `u8`, and lifetime `u8`.

Handle-transition subject kind `0` means parameter and `1` means result. Counts and indices are encoded little-endian. Descriptor
fields are emitted in callable order; handle transitions and buffer effects are emitted by subject kind and then ascending index.
The callable's direct `status` return is implicit and is not included in the result count.

Shape, direction, byte-access, lifetime, presence, handle-class, handle-state, call-mode, completion, cancellation-mode, capability,
and transition values use the closed ordinal tables defined by this section. The descriptor contains no role names, symbol
spellings, source paths, artifact paths, or host prose.

The semantic-contract digest is:

```text
BLAKE3(
    "bray.platform.role-contract\0"
    || little_endian_u64(canonical_descriptor_bytes.length)
    || canonical_descriptor_bytes
)
```

The numeric role identity binds the language-defined semantics in this catalog. Changing a role's observable semantics, ABI shape,
ownership transitions, buffer lifetime, completion behavior, or status interpretation without a compatible platform ABI change is
nonconforming. Binding validation compares the complete descriptor and digest before accepting a private declaration.

The descriptor records handle effects as ordered transition tuples. A borrowed input remains `borrowed` after immediate completion.
A start role changes each operation-retained input to `retained_borrow` only when it returns `Pending`, then restores `borrowed` at
terminal completion. A successful required owner result changes from `absent` to `owned`; it remains `absent` on failure. A
successful optional owner result changes to `owned` when nonzero and remains `absent` when zero. A close role changes its input from
`owned` to `consumed` on every terminal status. `operation.complete` changes its operation input to `retained_owner` while pending
and `consumed` when terminal. The wait-source result is `borrowed` and becomes invalid with terminal operation completion. These
rules cover every handle effect in the initial catalog and are part of the canonical descriptor.

### ABI Values

The private ABI uses fixed-width scalars, validated pointer-and-length borrows, opaque handles, and closed status values. It does not
exchange Rust, C++, or implementation-language object layouts. A provider cannot retain a borrowed buffer after an operation returns
unless the role contract explicitly transfers that borrow into a registered asynchronous operation whose lifetime is tied to a
checked frame or owner.

Caller-owned buffers cross the boundary by borrow. Providers do not allocate public Bray strings, collections, paths, errors, or
protocol values. Variable-length results use a size query, caller-supplied storage, or a bounded incremental operation. Numeric
platform codes remain uninterpreted data until a trusted Bray adapter maps them into stable public failure categories.

Opaque handles are target-local and process-local. They are not pointers in the public surface, cannot be forged by source, cannot
be serialized into compiled package interfaces, and cannot cross an independent process except through a separate declared
transfer contract. Every owned handle role identifies the operation that consumes or closes it.

### Binding And Validation

Private trusted standard-library declarations are explicitly associated with platform roles in product build metadata. The
compiler validates the role identity, callable shape, target availability, platform-service ABI compatibility, and semantic contract
before checking the binding and its safe wrappers. It never infers a role from a source path, declaration name, extern symbol, or
body implementation language.

Role associations stay private. Public `std` package interfaces contain only the ordinary inferred contracts of public wrappers.
They do not expose private role identities, symbols, handles, or native error codes as declaration identity.

Platform-service ABI compatibility uses one typed major and minor version for the complete closed contract. Consumers require an
equal major version and a provider minor version no older than the required minor version. Individual role families do not invent
independent version systems. A target artifact records the exact ABI version and roles it provides.

## Runtime Integration

The platform service ABI and protected-frame runtime ABI have separate ownership:

- platform roles provide external mechanisms and waitable completion sources,
- runtime roles schedule frames, register waits, wake tasks, propagate cancellation, and resolve run ownership.

An async standard-library wrapper joins the two through a private trusted Bray owner. Registration transfers the wait obligation to
that owner. Completion or cancellation removes the registration exactly once, resolves any retained buffer or handle dependency,
and wakes the suspended frame. A synchronous-only product can use blocking stream, file, clock, entropy, and child-process roles
without linking the async runtime.

No platform operation may call arbitrary Bray source without a role contract that establishes a valid callback execution root,
panic boundary, ownership transfer, and synchronization edge.

## Target Facts And Availability

The selected target profile exposes these boolean facts under `target.platform`:

```text
target.platform.process_context
target.platform.standard_streams
target.platform.filesystem
target.platform.child_processes
target.platform.monotonic_clock
target.platform.wall_clock
target.platform.entropy
```

These facts describe language-level availability. A true fact requires the selected target support artifacts to provide every
mandatory role for that service. A false fact makes the corresponding service operations unavailable during normal
target-conditional declaration checking.

Target facts do not select a provider or encode an artifact path. Exact provider identity, role bindings, ABI version, native
dependencies, and artifact digests remain build and link inputs.

## Artifacts And Linking

Each standard-library target artifact set records:

- the exact target identity,
- the platform-service ABI version,
- provided platform roles and their semantic-contract digest,
- the implementing static archive, direct system binding, or narrow native shim for each role,
- native dependency requirements,
- and content digests for every supplied artifact.

Direct platform bindings are preferred when the target exposes a stable representable ABI. A native shim is allowed only to
normalize mechanisms that cannot be expressed safely through direct declarations, such as macro-only APIs, unstable native
structures, unusual calling conventions, or signal and unwind trampolines. A shim cannot own portable Bray policy.

Compilation gathers required roles from reachable checked standard-library bindings. Product formation merges those requirements
deterministically, validates the selected provider, and publishes typed link-plan inputs. Unused service families do not force their
native artifacts or the async runtime into the product. The linker consumes the validated plan and does not rediscover roles from
unresolved symbols.

## Demand-Driven Compiler Facts

Platform service facts follow the compiler's ordinary lazy architecture:

1. Binding a public declaration requests its imported `std` declaration fact.
2. Consumer checking requests the public contract and target availability without exposing private role identities.
3. Building trusted standard-library source requests private role contracts only for bindings whose bodies are checked.
4. Lowering publishes the required operation or external call without choosing an artifact by filesystem search.
5. Product formation merges role requirements from reachable selected standard-library artifacts.
6. Code generation and link planning request the exact selected target artifact records.

Independent wrappers and role validations can run in parallel from immutable inputs. Required-role merging, diagnostics, and link
inputs use stable typed identities and canonical ordering so scheduling cannot affect the result.

## Diagnostics

Compile-time failures include unavailable target services, missing private roles, incompatible role signatures, incompatible ABI
versions, invalid semantic-contract records, missing target artifacts, and unsatisfied blocking or async execution requirements.

Compiler phases emit structured diagnostics with typed target, role, declaration, ABI-version, artifact, and source arguments.
`bray-messages` renders user-facing text. Platform error strings and native loader prose are never substituted for compiler
diagnostic messages.

Runtime I/O failures remain typed Bray values. Tooling can inspect their stable category and numeric platform code without requiring
localized compiler diagnostics.

## Conformance

The conformance suite must cover:

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
- deterministic requirement merging and diagnostics under parallel scheduling,
- and direct bindings and native shims producing equal public behavior.

Synthetic target providers must be sufficient for contract tests. Conformance tests must not depend on a machine-wide installation,
network service, locale-specific host message, or one operating system's handle representation.

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
