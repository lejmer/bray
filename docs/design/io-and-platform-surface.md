# I/O And Platform Standard-Library Surface

This document defines the concrete standard-library declaration surface for Bray's external I/O and platform facilities. The
language semantics governing these declarations are specified in
[I/O and platform services](../language/io-and-platform-services.md).

No I/O declaration is ambient. Source names it through an ordinary qualified path or makes it available through an explicit
`using` declaration.

## Package Layout

The public platform-facing modules are:

| Module | Public responsibility |
| --- | --- |
| `std.io` | Byte streams, standard input and output, buffering, flushing, and I/O failures |
| `std.path` | Lossless target-native paths and explicit text conversion |
| `std.fs` | Files, directories, metadata, traversal, and filesystem mutation |
| `std.process` | Process context, child-process control, and typed Bray child-process protocols |
| `std.time` | Durations, monotonic instants, wall-clock values, deadlines, and timers |
| `std.random` | System entropy and deterministic pseudorandom generation |

These modules can use private trusted declarations to reach the selected target. Private declarations are not public `std`
surface, are not available to user source, and do not change the semantics specified here.

The compiler does not recognize these modules by spelling. Their declarations remain ordinary standard-library declarations unless
another language rule explicitly identifies a particular declaration as compiler-known or recognized.

## Public Declaration Surface

The declarations in this section are the canonical public contract. The bodyless forms describe declaration surfaces and
are not source syntax that an ordinary package can use to omit a body. Fields named `internal state` represent private standard
library storage and are not part of the public package interface.

Changes to these modules must preserve the names, parameter modes, result shapes, ownership behavior, and blocking requirements
defined here unless the standard-library contract itself is revised.

### `std.io`

```bray
union IoErrorKind
{
    Unsupported;
    PermissionDenied;
    NotFound;
    AlreadyExists;
    InvalidInput;
    Interrupted;
    Exhausted;
    BrokenStream;
    TimedOut;
    Other;
}

struct IoError
{
    kind: IoErrorKind;
    platform_code: i64?;
    transferred: usize;
}

trait Reader
{
    mut func read(pos destination: &mut [u8]) -> Result<usize, IoError>
        requires(blocking_execution());
}

trait Writer
{
    mut func write(pos source: &[u8]) -> Result<usize, IoError>
        requires(blocking_execution());

    mut func flush() -> Result<unit, IoError>
        requires(blocking_execution());
}

trait AsyncReader
{
    mut async func read_async(pos destination: &mut [u8]) -> Result<usize, IoError>;
}

trait AsyncWriter
{
    mut async func write_async(pos source: &[u8]) -> Result<usize, IoError>;
    mut async func flush_async() -> Result<unit, IoError>;
}

union SeekFrom
{
    Start(pos offset: u64);
    Current(pos offset: i64);
    End(pos offset: i64);
}

trait Seeker
{
    mut func seek(pos from: SeekFrom) -> Result<u64, IoError>
        requires(blocking_execution());
}

trait AsyncSeeker
{
    mut async func seek_async(pos from: SeekFrom) -> Result<u64, IoError>;
}

struct StandardInput
{
    internal state: StandardInputState;
}

struct StandardOutput
{
    internal state: StandardOutputState;
}

struct StandardError
{
    internal state: StandardErrorState;
}

struct BufferedReader<Source>
{
    internal state: BufferedReaderState<Source>;
}

struct BufferedWriter<Sink>
{
    internal state: BufferedWriterState<Sink>;
}

impl BufferedReader<Source>
{
    consume func into_source() -> Source;
}

impl BufferedWriter<Sink>
{
    consume func into_sink() -> Result<Sink, IoError>;
}

func standard_input() -> StandardInput;
func standard_output() -> StandardOutput;
func standard_error() -> StandardError;

func buffered_reader<Source>(
    pos source: Source,
    capacity: usize,
) -> Result<BufferedReader<Source>, IoError>
    requires(capacity > 0);

func buffered_writer<Sink>(
    pos sink: Sink,
    capacity: usize,
) -> Result<BufferedWriter<Sink>, IoError>
    requires(capacity > 0);

func print(pos text: string) -> Result<unit, IoError>
    requires(blocking_execution());

func print_line(pos text: string) -> Result<unit, IoError>
    requires(blocking_execution());

async func print_async(pos text: string) -> Result<unit, IoError>;
async func print_line_async(pos text: string) -> Result<unit, IoError>;
```

`StandardInput` implements `Reader` and `AsyncReader`. `StandardOutput` and `StandardError` implement `Writer` and `AsyncWriter`.
These wrappers borrow product-lifetime standard streams and cannot close them. Buffered owners implement the matching traits of their
owned source or sink. Consuming a buffered writer through `into_sink` flushes it before returning its sink.

`IoError.transferred` is the number of bytes committed before the reported failure. It is zero for operations without byte
transfer. A caller must not retry the already transferred prefix.

### `std.path` and `std.fs`

The native text and path declarations belong to `std.path`. The remaining declarations belong to `std.fs` and therefore qualify
path types through `std.path`.

```bray
union PathError
{
    InvalidNativeValue;
    NotUtf8;
}

struct NativeText
{
    internal state: NativeTextState;
}

impl NativeText
{
    static func from_string(pos value: string) -> Result<NativeText, PathError>;
    func to_string() -> Result<string, PathError>;
}

struct Path
{
    internal state: PathState;
}

impl Path
{
    static func from_native(pos value: NativeText) -> Result<Path, PathError>;
    static func from_string(pos value: string) -> Result<Path, PathError>;
    func to_native() -> NativeText;
    func to_string() -> Result<string, PathError>;
    func join(pos child: &Path) -> Path;
    func normalize() -> Path;
}

union FileAccess
{
    Read;
    Write;
    ReadWrite;
    Append;
}

union FileCreation
{
    OpenExisting;
    CreateIfMissing;
    CreateNew;
    Truncate;
}

struct OpenOptions
{
    access: FileAccess;
    creation: FileCreation;
}

union FileKind
{
    File;
    Directory;
    Other;
}

struct FileMetadata
{
    kind: FileKind;
    bytes: u64;
    modified: std.time.Timestamp?;
}

struct File
{
    internal state: FileState;
}

impl File
{
    static func open(
        pos path: &std.path.Path,
        options: OpenOptions,
    ) -> Result<File, std.io.IoError>
        requires(blocking_execution());

    static async func open_async(
        pos path: &std.path.Path,
        options: OpenOptions,
    ) -> Result<File, std.io.IoError>;

    func metadata() -> Result<FileMetadata, std.io.IoError>
        requires(blocking_execution());

    async func metadata_async() -> Result<FileMetadata, std.io.IoError>;

    consume func close() -> Result<unit, std.io.IoError>
        requires(blocking_execution());

    consume async func close_async() -> Result<unit, std.io.IoError>;
}

struct DirectoryEntry
{
    path: std.path.Path;
    metadata: FileMetadata;
}

struct Directory
{
    internal state: DirectoryState;
}

impl Directory
{
    static func open(pos path: &std.path.Path) -> Result<Directory, std.io.IoError>
        requires(blocking_execution());

    static async func open_async(pos path: &std.path.Path) -> Result<Directory, std.io.IoError>;

    mut func next() -> Result<DirectoryEntry?, std.io.IoError>
        requires(blocking_execution());

    mut async func next_async() -> Result<DirectoryEntry?, std.io.IoError>;

    consume func close() -> Result<unit, std.io.IoError>
        requires(blocking_execution());

    consume async func close_async() -> Result<unit, std.io.IoError>;
}

func metadata(pos path: &std.path.Path) -> Result<FileMetadata, std.io.IoError>
    requires(blocking_execution());

async func metadata_async(pos path: &std.path.Path) -> Result<FileMetadata, std.io.IoError>;

func create_directory(pos path: &std.path.Path) -> Result<unit, std.io.IoError>
    requires(blocking_execution());

async func create_directory_async(pos path: &std.path.Path) -> Result<unit, std.io.IoError>;

func remove_file(pos path: &std.path.Path) -> Result<unit, std.io.IoError>
    requires(blocking_execution());

async func remove_file_async(pos path: &std.path.Path) -> Result<unit, std.io.IoError>;

func remove_directory(pos path: &std.path.Path) -> Result<unit, std.io.IoError>
    requires(blocking_execution());

async func remove_directory_async(pos path: &std.path.Path) -> Result<unit, std.io.IoError>;

func rename(pos source: &std.path.Path, pos destination: &std.path.Path) -> Result<unit, std.io.IoError>
    requires(blocking_execution());

async func rename_async(
    pos source: &std.path.Path,
    pos destination: &std.path.Path,
) -> Result<unit, std.io.IoError>;
```

`File` implements the blocking and asynchronous reader, writer, and seeker traits. A call disallowed by the file's `OpenOptions`
returns an `IoError` whose `kind` is `IoErrorKind.InvalidInput`. `Directory.next` and `next_async` return `Result.Ok(none)` only after
all entries have been observed.

Every asynchronous filesystem declaration has the same result and ownership effect as its blocking counterpart. A target can
implement it with a native completion source or a selected blocking lane, but that choice is not observable through the signature.
Every consuming `close` resolves its owner on both result variants. A close failure reports the failure but does not return a live
owner.

### `std.process`

```bray
struct Id
{
    internal state: ProcessIdState;
}

struct Arguments
{
    internal state: ArgumentsState;
}

impl Arguments
{
    func length() -> usize;
    func value_at(pos index: usize) -> std.path.NativeText?;
}

struct EnvironmentEntry
{
    key: std.path.NativeText;
    value: std.path.NativeText;
}

struct Environment
{
    internal state: EnvironmentState;
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

struct ChildInput
{
    internal state: ChildInputState;
}

struct ChildOutput
{
    internal state: ChildOutputState;
}

impl ChildInput
{
    consume func close() -> Result<unit, std.io.IoError>
        requires(blocking_execution());

    consume async func close_async() -> Result<unit, std.io.IoError>;
}

impl ChildOutput
{
    consume func close() -> Result<unit, std.io.IoError>
        requires(blocking_execution());

    consume async func close_async() -> Result<unit, std.io.IoError>;
}

struct ChildCommand
{
    internal state: ChildCommandState;
}

impl ChildCommand
{
    static func create(pos executable: std.path.Path) -> ChildCommand;
    mut func argument(pos value: std.path.NativeText);
    mut func environment_policy(policy: EnvironmentPolicy);
    mut func set_environment(pos key: std.path.NativeText, pos value: std.path.NativeText);
    mut func remove_environment(pos key: std.path.NativeText);
    mut func working_directory(pos path: std.path.Path);
    mut func standard_input(policy: ChildStreamPolicy);
    mut func standard_output(policy: ChildStreamPolicy);
    mut func standard_error(policy: ChildStreamPolicy);

    consume func spawn() -> Result<ChildProcess, ChildError>
        requires(blocking_execution());

    consume async func spawn_async() -> Result<ChildProcess, ChildError>;
}

struct ChildProcess
{
    internal state: ChildProcessState;
}

impl ChildProcess
{
    mut func take_standard_input() -> ChildInput?;
    mut func take_standard_output() -> ChildOutput?;
    mut func take_standard_error() -> ChildOutput?;

    mut func request_termination() -> Result<unit, ChildError>
        requires(blocking_execution());

    mut async func request_termination_async() -> Result<unit, ChildError>;

    consume func wait() -> Result<ExitStatus, ChildError>
        requires(blocking_execution());

    consume async func wait_async() -> Result<ExitStatus, ChildError>;

    consume func force_termination() -> Result<ExitStatus, ChildError>
        requires(blocking_execution());

    consume async func force_termination_async() -> Result<ExitStatus, ChildError>;
}
```

`ChildInput` implements `Writer` and `AsyncWriter`. `ChildOutput` implements `Reader` and `AsyncReader`. A piped handle can be taken
at most once. Inherited and null policies produce no public pipe owner. Their consuming `close` operations resolve the pipe owner on
both result variants.

`request_termination` and `request_termination_async` request the target's cooperative termination mechanism without consuming the
owner, waiting, or reaping. Every returning `ChildProcess.wait`, `wait_async`, `force_termination`, or `force_termination_async` path
has reaped the child and resolved the owner, including `Result.Error`. An infrastructure failure detected before reaping is retained
while cleanup continues and is returned only after ownership has been resolved. An unresolved owner has fallible asynchronous
finalization, so normal scope exit requires an explicit consuming completion operation.

Cancellation of a raw async consuming completion requests forced termination, waits and reaps under cancellation shielding, resolves
all pipe owners retained by the child owner, and then forwards cancellation to the current run. It never returns a `ChildError`
after cancellation propagation and never leaves a child detached implicitly.

The typed `Process<T>` declarations specified by the concurrency chapter remain separate from `ChildProcess`. They use
`ChildProcess` internally and add the authenticated Bray protocol and `RunResult<T>` contract.

### `std.time`

```bray
struct Duration
{
    internal state: DurationState;
}

impl Duration
{
    static func from_parts(seconds: u64, nanoseconds: u32) -> Result<Duration, ClockError>;
    func seconds() -> u64;
    func nanoseconds() -> u32;
}

struct Instant
{
    internal state: InstantState;
}

impl Instant
{
    func duration_since(pos earlier: &Instant) -> Result<Duration, ClockError>;
}

struct Timestamp
{
    internal state: TimestampState;
}

impl Timestamp
{
    func seconds() -> i64;
    func nanoseconds() -> u32;
}

union ClockError
{
    Unavailable;
    OutOfRange;
    Platform(pos error: std.io.IoError);
}

func monotonic_now() -> Result<Instant, ClockError>;
func wall_now() -> Result<Timestamp, ClockError>;

func sleep(pos duration: Duration) -> Result<unit, ClockError>
    requires(blocking_execution());

async func sleep_async(pos duration: Duration) -> Result<unit, ClockError>;
```

`Duration.nanoseconds()` and `Timestamp.nanoseconds()` are less than one billion. Construction and arithmetic reject values outside
their representable range. `Instant.duration_since` returns `ClockError.OutOfRange` when `earlier` is later or belongs to a different
monotonic clock identity. `sleep_async` returns normally only after its monotonic deadline and forwards current-run cancellation only
after its timer registration has been removed.

### `std.random`

```bray
union EntropyError
{
    Unavailable;
    Platform(pos error: std.io.IoError);
}

func fill_entropy(pos destination: &mut [u8]) -> Result<unit, EntropyError>
    requires(blocking_execution());

async func fill_entropy_async(pos destination: &mut [u8]) -> Result<unit, EntropyError>;

struct Generator
{
    internal state: GeneratorState;
}

impl Generator
{
    static func seeded(pos seed: [u8; 32]) -> Generator;

    static func from_entropy() -> Result<Generator, EntropyError>
        requires(blocking_execution());

    static async func from_entropy_async() -> Result<Generator, EntropyError>;

    mut func fill(pos destination: &mut [u8]);
    mut func next_u64() -> u64;
}
```

`fill_entropy` and `fill_entropy_async` either initialize the entire destination or return an error whose nested
`IoError.transferred` states the initialized prefix. `Generator` has one specification-defined deterministic algorithm. Equal seeds
produce equal byte and `u64` sequences on every target. No generator method consults system entropy after construction.

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
[standard-library concurrency and parallelism](../language/async-and-concurrency/standard-library-concurrency.md). Arbitrary external programs
use an explicit exit representation and are not treated as Bray runs.

## Time

`std.time` distinguishes durations, monotonic instants, and wall-clock timestamps.

A monotonic instant is meaningful only within the process and clock domain that produced it. Later readings from that domain do not
precede earlier readings. Monotonic instants can measure elapsed time and establish deadlines, but they are not calendar timestamps
and are not portable serialized values.

A wall-clock value represents the target's civil-time source. It can move forward or backward when that source is adjusted.
Algorithms requiring elapsed-time ordering use the monotonic clock instead.

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

- [I/O and platform language semantics](../language/io-and-platform-services.md)
- [I/O and platform implementation architecture](io-and-platform-services.md)
- [Standard library design](standard-library.md)
