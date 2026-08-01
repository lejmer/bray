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
mandatory role for that service. A false fact makes the corresponding service operations unavailable during normal target-
conditional declaration checking.

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
