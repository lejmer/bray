# I/O and platform services

Bray exposes external I/O and platform facilities through ordinary declarations in the `std` package. These declarations obey
normal visibility, ownership, checking, target-availability, and package-interface rules.

No I/O declaration is ambient. Source names it through an ordinary qualified path or makes it available through an explicit
`using` declaration.

## Service Modules

The public platform-facing modules are:

| Module        | Public responsibility                                                          |
|---------------|--------------------------------------------------------------------------------|
| `std.io`      | Byte streams, standard input and output, buffering, flushing, and I/O failures |
| `std.path`    | Lossless target-native paths and explicit text conversion                      |
| `std.fs`      | Files, directories, metadata, traversal, and filesystem mutation               |
| `std.process` | Process context, child-process control, and typed Bray child-process protocols |
| `std.time`    | Durations, monotonic instants, wall-clock values, deadlines, and timers        |
| `std.random`  | System entropy and deterministic pseudorandom generation                       |

These modules can use internal trusted declarations to reach the selected target. Internal declarations are not public `std`
surface, require explicit internal-use acknowledgement outside their intended scope, and do not change the semantics specified
here.

The compiler does not recognize these modules by spelling. Their declarations remain ordinary standard-library declarations unless
another language rule explicitly identifies a particular declaration as compiler-known or recognized.

## Byte Streams

`std.io` readers and writers transfer bytes. Text I/O is built by explicitly encoding or decoding text through the standard text
contracts.

A read can produce fewer bytes than the supplied destination can hold. Reading zero bytes into a non-empty destination means that
the stream has reached its end. A write can accept fewer bytes than supplied, and callers or buffering wrappers continue until the
requested sequence is committed or a typed failure is returned.

Flush is an explicit operation. Successful flush means that bytes buffered by the Bray wrapper have been handed to the underlying
stream according to that stream's contract. It does not promise physical persistence unless the specific resource operation also
provides that guarantee.

Standard input, standard output, and standard error are process-root resources. Their handles are nonforgeable values. Concurrent
access follows the synchronization contract of the standard-library wrapper and does not permit an unsynchronized data race.

## Paths And Filesystems

A `std.path` value preserves one target-native path losslessly. A path is not required to be valid UTF-8. Converting text into a
path validates the selected target's path restrictions, and converting a path into text is fallible when the native value has no
lossless UTF-8 representation.

Path value equality and ordering operate on the exact native representation through a target-defined total ordering. They do not
query the filesystem or claim that two differently represented paths identify the same object. Lexical normalization is explicit
and produces another path value.

Filesystem resources are owned values. Opening a file or directory transfers one resource obligation into the returned owner.
Copying the owner does not duplicate the platform resource. Explicit consuming close or completion resolves the obligation.

If resource finalization can fail, normal scope exit cannot silently discard that failure. Source must use a lifecycle path whose
contract handles or propagates the result. Abnormal exit follows the ordinary cleanup-incident rules.

Relative filesystem paths resolve against the process-start working-directory snapshot. Bray does not provide an operation that
mutates a process-wide current directory. A child process can instead receive an explicit working directory in its construction
request.

An API that promises deterministic directory traversal orders entries by the target path ordering before exposing them. An
incremental native-order traversal can explicitly leave ordering unspecified. Source requiring reproducible behavior must choose a
deterministic traversal or sort the results.

## Process Context

The executable root receives one immutable process-context snapshot containing:

- process identity,
- process arguments,
- environment entries,
- the startup working directory,
- and standard-stream capabilities.

Public `std.process` accessors observe this snapshot. Changes made to host-global process state after the snapshot was formed are not
visible through it.

Arguments and environment entries preserve target-native units losslessly. Their conversion to and from UTF-8 text is explicit and
fallible where the selected target cannot represent the requested value.

Environment lookup and duplicate-key validation use the selected target's environment-key comparison rules. Iteration preserves
the immutable snapshot but does not promise source ordering unless the caller requests a deterministically ordered view.

Bray does not expose mutation of the current process environment. A child-process request builds an explicit environment from an
empty set or the immutable startup snapshot and applies explicit additions and removals. Inheritance is therefore a declared
request policy rather than hidden ambient behavior.

## Child Processes

`std.process` distinguishes raw child-process control from the typed Bray process protocol.

A raw child-process owner controls one operating-system child. Construction specifies the executable, arguments, environment,
working directory, and standard-stream policy. The owner remains responsible for the child until it has terminated and been
reaped.

Waiting, requesting cooperative termination, forcing termination where supported, and reaping are distinct operations. A
termination request does not itself resolve ownership. An error path cannot abandon a live or unreaped child.

Process creation, capacity, permission, transport, signalling, waiting, and reaping failures are typed operational failures. A
normal nonzero exit status or target termination status is process outcome data rather than a compiler panic.

The higher-level `std.process.Process<T>` contract adds authenticated Bray protocol behavior and yields `RunResult<T>` only for a
conforming Bray child. Its encoding, decoding, panic-report, cancellation, and commit-ordering rules are defined by
[standard-library concurrency and parallelism](async-and-concurrency/standard-library-concurrency.md). Arbitrary external programs
use an explicit exit representation and are not treated as Bray runs.

## Time

`std.time` distinguishes exact durations, monotonic instants, absolute timestamps, local calendar values, UTC offsets, and named
time zones. These concepts do not convert implicitly.

A monotonic instant is meaningful only within the process and clock domain that produced it. Readings from that domain do not
precede earlier readings. Monotonic instants can measure elapsed time and establish deadlines, but they are not calendar timestamps
and are not portable serialized values.

A wall-clock reading produces an absolute timestamp. It can move forward or backward when the host clock is adjusted. Algorithms
requiring elapsed-time ordering use the monotonic clock instead.

A date and local date-time use the proleptic Gregorian calendar without identifying an instant. Invalid fields are rejected rather
than normalized. A UTC offset identifies one fixed displacement from UTC but is not a named timezone. A named timezone uses the
IANA timezone rules shipped with the selected Bray toolchain.

Converting an absolute timestamp through a timezone always produces one local date-time and offset. Converting a local date-time
through a timezone explicitly reports whether it is unique, occurs twice during a backward transition, or does not occur during a
forward transition. The general conversion never silently chooses an earlier or later interpretation.

Exact durations and calendar periods are different types. Exact duration arithmetic changes an instant by a fixed elapsed amount.
Calendar-period arithmetic changes calendar fields and can require an explicit invalid-date or timezone-transition policy. Adding
24 hours is not assumed to be equivalent to adding one local calendar day.

Strict RFC 3339 and ISO 8601 interchange is locale-independent. Named-zone behavior and timezone aliases use the toolchain's pinned
timezone database, so compilation and execution do not depend on the host's database version or perform runtime downloads.

Clock resolution is explicit target information. Arithmetic detects overflow and does not silently wrap. Reading either clock is
an I/O effect and can return a typed failure when the selected service cannot provide a valid reading.

Blocking waits require `blocking_execution()`. Async timers suspend through the selected runtime's wait integration and do not
block a cooperative execution lane.

## Entropy And Random Generation

System entropy is an external nondeterministic input. Requesting it is an I/O effect and can fail. Failure never causes an implicit
fallback to time, process identity, memory addresses, or another predictable seed.

Deterministic pseudorandom generators are ordinary owned Bray values. Equal generator algorithm identity and seed state produce the
same output sequence. Their methods do not read system entropy implicitly.

A convenience that creates a generator from system entropy is explicitly fallible and nondeterministic. APIs that need reproducible
results accept an explicit generator or seed rather than consulting ambient state.

## Effects And Execution Requirements

Files, terminals, processes, clocks, environment state, and entropy are external state under the language's I/O effect rules.
Effect-free contexts cannot access them.

An operation that can block its operating-system thread requires `blocking_execution()`. An async wrapper must suspend or arrange
work on a compatible blocking lane. Declaring a function `async` does not make a blocking platform operation nonblocking.

I/O owners and values carry ordinary ownership, borrowing, dependency, affinity, capability, lifecycle, and finalization contracts.
Passing a resource or dependent buffer across a task, native thread, process, callback, or suspension boundary is valid only when
the destination preserves those contracts.

## Failures And Diagnostics

Expected external failures are typed Bray values returned through `Result`. Public failure values expose stable categories and can
retain target-specific numeric codes for inspection. Host-authored prose is not a stable semantic value.

Compiler errors such as using an unavailable target service, calling a blocking operation without `blocking_execution()`, or
linking a product without a required platform service are structured compiler diagnostics. Their localized text is not part of the
runtime failure value.

Malformed data, denied access, missing paths, exhausted process capacity, interrupted operations, unavailable clocks, and entropy
failure do not permit the compiler or standard library to panic. A panic is reserved for a violated language or trusted-boundary
invariant.

## Target Availability

The following compiler-known boolean target facts describe platform-service availability:

```bray
target.platform.process_context
target.platform.standard_streams
target.platform.filesystem
target.platform.child_processes
target.platform.monotonic_clock
target.platform.wall_clock
target.platform.entropy
```

The corresponding service operations are target-conditional declarations. Using an unavailable operation is a compile-time error.
A target-disabled module contribution can exclude unavailable operations through the normal target-gating rules.

A true fact promises the complete service semantics required by this specification. It does not reveal a native symbol, artifact
path, operating-system handle representation, or implementation language.

## Navigation

- [Language index](index.md)
- [Compiler-known declarations and standard library recognition](compiler-known-and-standard-library.md)
- [Async and concurrency](async-and-concurrency.md)
- [Targets, layout, ABI, and raw memory](targets-layout-abi-and-raw-memory.md)
