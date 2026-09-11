# I/O and platform services

Bray exposes portable external I/O, process, time, and entropy facilities through ordinary declarations in the `std`
package. These declarations obey normal visibility, ownership, checking, target-availability, and package-interface
rules.

No I/O declaration is ambient. Source names it through an ordinary qualified path or makes it available through an
explicit `using` declaration.

## Service Modules

The portable public service modules specified by this chapter are:

| Module        | Public responsibility                                                          |
|---------------|--------------------------------------------------------------------------------|
| `std.io`      | Byte streams, standard input and output, buffering, flushing, and I/O failures |
| `std.path`    | Lossless target-native paths and explicit text conversion                      |
| `std.fs`      | Files, directories, metadata, traversal, and filesystem mutation               |
| `std.process` | Process context, child-process control, and typed Bray child-process protocols |
| `std.time`    | Durations, monotonic instants, wall-clock values, deadlines, and timers        |
| `std.random`  | System entropy and deterministic pseudorandom generation                       |

Other public `std` modules remain outside this chapter. `std.atomic`, `std.run`, and `std.task` belong to
[standard-library concurrency and parallelism](async-and-concurrency/standard-library-concurrency.md). `std.memory`
belongs to [the standard-library memory surface](targets-layout-abi-and-raw-memory/standard-library-memory-surface.md).
`std.ffi`, `std.ffi.c`, `std.dynamic`, and the target-specific `std.os.*` modules belong to
[targets, layout, ABI, and raw memory](targets-layout-abi-and-raw-memory.md). Core data and testing modules are ordinary
standard-library surfaces rather than platform services specified here.

`std.platform` is an internal implementation module, not part of the public package surface.

These modules can use internal trusted declarations to reach the selected target. Internal declarations are not public
`std` surface, require explicit internal-use acknowledgement outside their intended scope, and do not change the
semantics specified here.

The compiler does not recognize these modules by spelling. Their declarations remain ordinary standard-library
declarations unless another language rule explicitly identifies a particular declaration as compiler-known or
recognized.

## Byte Streams

`std.io` readers and writers transfer bytes. Text I/O is built by explicitly encoding or decoding text through the
standard text contracts.

A read can produce fewer bytes than the supplied destination can hold. Reading zero bytes into a non-empty destination
means that the stream has reached its end. A write can accept fewer bytes than supplied, and callers or buffering
wrappers continue until the requested sequence is committed or a typed failure is returned.

The public I/O surface has three levels. `print` and `print_line` are the ordinary text-output operations. They accept a
copyable `string` value, so literals and existing string bindings need no explicit borrow. Their `IoError` result can be
ignored for routine output or handled when the program cares about output failure. `write_all`, `write_all_async`,
`read_exact`, and `read_exact_async` expose complete byte-transfer failures and exact progress. The `Reader`, `Writer`,
`AsyncReader`, and `AsyncWriter` operations expose individual partial transfers for buffering, streaming, and resource
adapters.

```bray
std.io.print_line("ready");

try std.io.print_line(message);
```

`Writer.write_all` and `AsyncWriter.write_all_async` are default trait methods that commit a complete borrowed byte
slice through the required partial-write operation. `Reader.read_exact` and `AsyncReader.read_exact_async` are default
trait methods that initialize a complete borrowed destination through the required partial-read operation. They retry
partial transfers and return an `IoError` whose `transferred` value is the
exact completed prefix when a transfer fails. Reaching the end of a stream before `read_exact` fills its destination is
a `BrokenStream` failure with that same exact progress. A stream result that claims more bytes than the offered range is
an invalid transfer failure.

Buffered readers and writers use one owned fixed-capacity staging area. Small writes coalesce until the staging area
must be drained or the caller explicitly flushes. Draining staged bytes does not flush the underlying resource. A write
at least as large as the staging capacity bypasses an empty staging area and transfers directly from the caller's
borrowed slice. A read into a destination at least as large as the staging capacity bypasses an empty staging area and
transfers directly into the caller's borrowed slice. These rules apply equally to synchronous and asynchronous forms.
Bulk staging transfers use the standard memory copy operations, while direct transfers add no intermediate allocation
or full-payload copy.

Buffered construction returns the supplied source or sink alongside `IoError` on failure. `into_sink` and
`into_sink_async` flush before transferring the sink. On failure, they return the buffered writer alongside
`IoError`. The returned writer retains the remaining buffered bytes and the sink's updated state, so retry continues
after the committed prefix.

Flush is an explicit operation. Successful flush means that bytes buffered by the Bray wrapper have been handed to the
underlying stream according to that stream's contract. It does not promise physical persistence unless the specific
resource operation also provides that guarantee.

Standard input, standard output, and standard error are process-root resources. Their handles are nonforgeable values.
Concurrent access follows the synchronization contract of the standard-library wrapper and does not permit an
unsynchronized data race.

Each standard-input, standard-output, and standard-error operation has one guard owned by the concrete standard stream.
Every `StandardInput.read` call holds the input guard for the complete read, so independently obtained input values
coordinate through the same process-root owner. A `print` or
`print_line` call holds that guard across every partial-write retry, the optional line ending, and the final flush. A
direct `Writer.write` or `Writer.flush` call on a standard stream is one operation with its own guard. Synchronous and
asynchronous forms use the same boundary.

The native write and flush leaves require an active guard and add no synchronization of their own. Generic writer,
formatting, and buffering code also add no stream guard. A buffered standard stream therefore acquires the concrete
stream guard only when it transfers or flushes bytes, and its flush and destruction paths do not recursively acquire a
guard already held by the buffer.

The guard ends on success, returned error, panic, cancellation, and ordinary destruction. Captured, discarded, and
inherited standard streams use the same operation boundary while owning independent destination mechanisms. Files and
process pipes are owned resources accessed through exclusive mutation, so their native leaves add no standard-stream
guard.

## Paths And Filesystems

A `std.path` value preserves one target-native path losslessly. A path is not required to be valid UTF-8. Converting
text into a path validates the selected target's path restrictions, and converting a path into text is fallible when the
native value has no lossless UTF-8 representation.

Path value equality and ordering operate on the exact native representation through a target-defined total ordering.
They do not query the filesystem or claim that two differently represented paths identify the same object. Lexical
normalization is explicit and produces another path value.

Filesystem resources are non-copyable owned values. Opening a file or directory transfers one resource obligation
into the returned owner. `File.close` and `Directory.close`, including their asynchronous forms, borrow their owner
mutably. Success establishes the owner's `complete` predicate. After an error, the owner retains any resource that
the platform still holds, so the caller can retry, transfer ownership, or handle an already-completed outcome.

The `complete` predicate records whether the resource obligation has been resolved. A platform operation can release
its resource while reporting an error. That error remains observable and the completed owner is safe to destroy.
`File.is_complete()` and `Directory.is_complete()` expose this state to ordinary code. Their checked postconditions
connect the returned Boolean to the owner's completion predicate, including after a close error.
Operations that require an open file or directory return an error when called on a completed owner. Ordinary scope
exit uses [completion proofs](lifecycle/finalization.md). Abnormal exit follows the cleanup-incident rules.

Relative filesystem paths resolve against the process-start working-directory snapshot. Bray does not provide an
operation that mutates a process-wide current directory. A child process can instead receive an explicit working
directory in its construction request.

An API that promises deterministic directory traversal orders entries by the target path ordering before exposing them.
An incremental native-order traversal can explicitly leave ordering unspecified. Source requiring reproducible behavior
must choose a deterministic traversal or sort the results.

## Process Context

The executable root receives one immutable process-context snapshot containing:

- process identity,
- process arguments,
- environment entries,
- and the startup working directory.

Standard input, standard output, and standard error are separate process-root resources obtained through `std.io`. They
are not encoded as interchangeable handles in the process-context snapshot.

Public `std.process` accessors observe this snapshot. Changes made to host-global process state after the snapshot was
formed are not visible through it.

Arguments and environment entries preserve target-native units losslessly. Their conversion to and from UTF-8 text is
explicit and fallible where the selected target cannot represent the requested value.

Environment lookup and duplicate-key validation use the selected target's environment-key comparison rules. Indexed
iteration is ordered lexicographically by the unsigned target-native code units of each key and is deterministic. This
ordering does not replace the target's key-equality rule.

Bray does not expose mutation of the current process environment. A child-process request builds an explicit environment
from an empty set or the immutable startup snapshot and applies explicit additions and removals. Inheritance is
therefore a declared request policy rather than hidden ambient behavior.

`std.process.Id`, `Arguments`, and `Environment` are nonforgeable value types with no public primary construction.
`EnvironmentEntry` is an ordinary owned product. The public process-context declarations are:

```bray
struct EnvironmentEntry
{
    key: std.path.NativeText;
    value: std.path.NativeText;
}

impl Arguments
{
    func length() -> usize;
    func value_at(pos index: usize) -> std.path.NativeText?;
}

impl Environment
{
    func length() -> usize;
    func value(pos key: &std.path.NativeText) -> std.path.NativeText?;
    func entry_at(pos index: usize) -> EnvironmentEntry?;
}

func current_id() -> Id;
func arguments() -> Arguments;
func environment() -> Environment;
func current_directory() -> std.path.Path;
```

`arguments()` excludes the executable path. Indexing is zero-based and `value_at` returns `none` outside the snapshot.
`Environment.value` uses target environment-key equality. `Environment.entry_at` uses that code-unit order, making
iteration deterministic and independent of host enumeration order. `current_directory()` returns the startup
working-directory snapshot and never observes later host-global mutation. Two `Id` values are equal exactly when they
identify the same process lifetime in the same host observation domain.

## Child Processes

`std.process` distinguishes raw child-process control from the typed Bray process protocol.

A raw child-process owner controls one operating-system child. Construction specifies the executable, arguments,
environment, working directory, and standard-stream policy. The owner remains responsible for the child until it has
terminated and been reaped.

Waiting, requesting cooperative termination, forcing termination where supported, and reaping are distinct operations. A
termination request does not itself resolve ownership. An error path cannot abandon a live or unreaped child.

Process creation, capacity, permission, transport, signalling, waiting, and reaping failures are typed operational
failures. A normal nonzero exit status or target termination status is process outcome data rather than a compiler
panic.

The higher-level `std.process.Process<T>` contract adds authenticated Bray protocol behavior and yields `RunResult<T>`
only for a conforming Bray child. Its encoding, decoding, panic-report, cancellation, and commit-ordering rules are
defined by [standard-library concurrency and parallelism](async-and-concurrency/standard-library-concurrency.md).
Arbitrary external programs use an explicit exit representation and are not treated as Bray runs.

`ChildCommand` is the mutable construction request. `ChildProcess`, `ChildInput`, and `ChildOutput` are nonforgeable
owners with no public primary construction. The public raw child-process declarations are:

```bray
union ChildStreamPolicy
{
    Inherit;
    Null;
    Piped;
}

union EnvironmentPolicy
{
    Empty;
    InheritSnapshot;
}

union ChildErrorKind
{
    Creation;
    Waiting;
    Termination;
    Reaping;
}

struct ChildError
{
    kind: ChildErrorKind;
    io: std.io.IoError;
}

union ExitStatus
{
    Code(pos code: i32);
    TargetTermination(pos code: i64);
}

impl ChildInput
{
    mut func close() -> Result<unit, std.io.IoError>
        requires(blocking_execution());
}

impl ChildOutput
{
    mut func close() -> Result<unit, std.io.IoError>
        requires(blocking_execution());
}

impl ChildCommand
{
    construct(pos executable: std.path.Path) -> Result<Self, ChildError>;
    mut func argument(pos value: std.path.NativeText) -> Result<unit, ChildError>;
    mut func environment_policy(policy: EnvironmentPolicy) -> Result<unit, ChildError>;

    mut func set_environment(
        pos key: std.path.NativeText,
        pos value: std.path.NativeText,
    ) -> Result<unit, ChildError>;

    mut func remove_environment(pos key: &std.path.NativeText) -> unit;
    mut func working_directory(pos path: std.path.Path) -> unit;
    mut func standard_input(policy: ChildStreamPolicy) -> unit;
    mut func standard_output(policy: ChildStreamPolicy) -> unit;
    mut func standard_error(policy: ChildStreamPolicy) -> unit;

    consume func spawn() -> Result<ChildProcess, ChildError>
        requires(blocking_execution());
}

impl ChildProcess
{
    func id() -> Id;

    mut func take_standard_input() -> ChildInput?;
    mut func take_standard_output() -> ChildOutput?;
    mut func take_standard_error() -> ChildOutput?;

    mut func request_termination() -> Result<unit, ChildError>
        requires(blocking_execution());

    mut func wait() -> Result<ExitStatus, ChildError>
        requires(blocking_execution());

    mut func force_termination() -> Result<ExitStatus, ChildError>
        requires(blocking_execution());
}
```

`ChildCommand` begins with no child arguments, `EnvironmentPolicy.InheritSnapshot`, no explicit working directory, and
`ChildStreamPolicy.Inherit` for every standard stream. No explicit working directory means the current process's startup
working-directory snapshot. A piped handle can be taken at most once. Inherited and null policies produce no pipe owner.

`ChildInput` implements `std.io.Writer`. `ChildOutput` implements `std.io.Reader`. Their mutably borrowing `close`
operations establish `complete` on success and preserve the platform's resource disposition on error.
`request_termination` requests termination while retaining the process owner. Successful `wait` or `force_termination`
reaps the child, closes retained pipes, and establishes `ChildProcess.complete`. An error retains unresolved resources
in the borrowed owner. The caller can retry or transfer that owner. Each pipe and process owner exposes
`is_complete()`, whose checked postconditions establish whether its completion predicate holds. This also lets
callers handle an operation that reports an error after releasing its resources. Normal scope exit requires a
completion proof for each retained owner.
`ChildProcess.id()` is observational and grants no termination, waiting, raw-handle, or shared-memory authority
independently of the owner.

`spawn` does not return an error while retaining an unowned live child. If failure occurs after operating-system
creation, it forces termination where necessary and reaps before returning. If cancellation of the calling run is
observed during `wait` or `force_termination`, the operation forces termination, reaps, and resolves retained
pipes under shielding before continuing caller cancellation. A cleanup failure becomes a cleanup incident rather than
abandoning the child.

## Time

`std.time` distinguishes exact durations, monotonic instants, absolute timestamps, local calendar values, UTC offsets,
and named time zones. These concepts do not convert implicitly.

A monotonic instant is meaningful only within the process and clock domain that produced it. Readings from that domain
do not precede earlier readings. Monotonic instants can measure elapsed time and establish deadlines, but they are not
calendar timestamps and are not portable serialized values.

A wall-clock reading produces an absolute timestamp. It can move forward or backward when the host clock is adjusted.
Algorithms requiring elapsed-time ordering use the monotonic clock instead.

A date and local date-time use the proleptic Gregorian calendar without identifying an instant. Invalid fields are
rejected rather than normalized. A UTC offset identifies one fixed displacement from UTC but is not a named timezone. A
named timezone uses the IANA timezone rules shipped with the selected Bray toolchain.

Converting an absolute timestamp through a timezone always produces one local date-time and offset. Converting a local
date-time through a timezone explicitly reports whether it is unique, occurs twice during a backward transition, or does
not occur during a forward transition. The general conversion never silently chooses an earlier or later interpretation.

Exact durations and calendar periods are different types. Exact duration arithmetic changes an instant by a fixed
elapsed amount. Calendar-period arithmetic changes calendar fields and can require an explicit invalid-date or
timezone-transition policy. Adding 24 hours is not assumed to be equivalent to adding one local calendar day.

Strict RFC 3339 and ISO 8601 interchange is locale-independent. Named-zone behavior and timezone aliases use the
toolchain's pinned timezone database, so compilation and execution do not depend on the host's database version or
perform runtime downloads.

```bray
let date = try std.time.Date(year = 2026, month = 8, day = 30);
let time = try std.time.TimeOfDay(14, 45, 0, nanosecond = 125000000);
let local = std.time.LocalDateTime(date, time);
let timestamp = try std.time.Timestamp(seconds = 0);
let parsed = try std.time.Timestamp.parse(&"2026-08-30T14:45:00.125Z");
```

`Date`, `TimeOfDay`, `LocalDateTime`, and `Timestamp` use primary constructors for component values. Defaulted
nanosecond parameters remove redundant arity variants. Their `parse` constructors keep strict ISO 8601 and RFC 3339 text
parsing visible at call sites.

Clock resolution is explicit target information. Arithmetic detects overflow and does not silently wrap. Reading either
clock is an I/O effect and can return a typed failure when the selected service cannot provide a valid reading.

Blocking waits require `blocking_execution()`. An async wait can defer that requirement into its future so that
`start()` selects a compatible blocking lane. Directly awaiting such a future requires the current lane to permit
blocking. An async wait implemented through runtime timer integration can instead suspend its current task without
requiring a blocking lane.

## Entropy And Random Generation

System entropy is an external nondeterministic input. Requesting it is an I/O effect and can fail. Failure never causes
an implicit fallback to time, process identity, memory addresses, or another predictable seed.

Deterministic pseudorandom generators are ordinary owned Bray values. Equal generator algorithm identity and seed state
produce the same output sequence. Their methods do not read system entropy implicitly.

A convenience that creates a generator from system entropy is explicitly fallible and nondeterministic. APIs that need
reproducible results accept an explicit generator or seed rather than consulting ambient state.

## Effects And Execution Requirements

Files, terminals, processes, clocks, environment state, and entropy are external state under the language's I/O effect
rules. Effect-free contexts cannot access them.

An operation that can block its operating-system thread requires `blocking_execution()`. An async wrapper must suspend
or arrange work on a compatible blocking lane. Declaring a function `async` does not make a blocking platform operation
nonblocking.

I/O owners and values carry ordinary ownership, borrowing, dependency, affinity, capability, lifecycle, and finalization
contracts. Passing a resource or dependent buffer across a task, native thread, process, callback, or suspension
boundary is valid only when the destination preserves those contracts.

## Failures And Diagnostics

Expected external failures are typed Bray values returned through `Result`. Public failure values expose stable
categories and can retain target-specific numeric codes for inspection. Host-authored prose is not a stable semantic
value.

Compiler errors such as using an unavailable target service, calling a blocking operation without
`blocking_execution()`, or linking a product without a required platform service are structured compiler diagnostics.
Their localized text is not part of the runtime failure value.

Malformed data, denied access, missing paths, exhausted process capacity, interrupted operations, unavailable clocks,
and entropy failure do not permit the compiler or standard library to panic. A panic is reserved for a violated language
or trusted-boundary invariant.

## Target Availability

The following compiler-known boolean target properties describe platform-service availability:

```bray
target.platform.process_context
target.platform.standard_streams
target.platform.filesystem
target.platform.child_processes
target.platform.monotonic_clock
target.platform.wall_clock
target.platform.entropy
target.platform.dynamic_loading
```

The corresponding service operations are target-conditional declarations. Using an unavailable operation is a
compile-time error. A target-disabled module contribution can exclude unavailable operations through the normal
target-gating rules.

A true condition promises the complete service semantics required by this specification. It does not reveal a native
symbol, artifact path, operating-system handle representation, or implementation language.

`target.platform.dynamic_loading` promises the complete closed dynamic-library role family defined by the
platform-service design contract. It does not permit ambient path search or make dynamically loaded code part of the
package graph.

## Navigation

- [Language index](index.md)
- [Compiler-known declarations and standard library recognition](compiler-known-and-standard-library.md)
- [Async and concurrency](async-and-concurrency.md)
- [Targets, layout, ABI, and raw memory](targets-layout-abi-and-raw-memory.md)
