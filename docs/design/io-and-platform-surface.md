# I/O And Platform Standard-Library Surface

This document defines the concrete standard-library declaration surface for Bray's portable external I/O, process, time,
and entropy facilities. The language semantics governing these declarations are specified in
[I/O and platform services](../language/io-and-platform-services.md).

No I/O declaration is ambient. Source names it through an ordinary qualified path or makes it available through an
explicit `using` declaration.

## Package Layout

The portable public service modules whose declaration contracts are defined here are:

| Module        | Public responsibility                                                          |
|---------------|--------------------------------------------------------------------------------|
| `std.io`      | Byte streams, standard input and output, buffering, flushing, and I/O failures |
| `std.path`    | Lossless target-native paths and explicit text conversion                      |
| `std.fs`      | Files, directories, metadata, traversal, and filesystem mutation               |
| `std.process` | Process context, child-process control, and typed Bray child-process protocols |
| `std.time`    | Durations, monotonic instants, wall-clock values, deadlines, and timers        |
| `std.random`  | System entropy and deterministic pseudorandom generation                       |

Other public modules in the `std` package are intentionally outside this document. Core data modules are defined by
[Core data standard library](core-data-standard-library.md). `std.atomic`, `std.run`, and `std.task` are defined by the
[concurrency specification](../language/async-and-concurrency/standard-library-concurrency.md). `std.memory` is defined
by the
[standard-library memory surface](../language/targets-layout-abi-and-raw-memory/standard-library-memory-surface.md).
`std.ffi`, `std.ffi.c`, `std.dynamic`, and the target-specific `std.os.*` modules are defined by
[Foreign and platform interoperability](foreign-and-platform-interoperability.md). `std.testing` is defined by
[Testing standard library and runner](testing.md).

`std.platform` is an internal implementation module, not part of the public package surface.

These modules can use internal trusted declarations to reach the selected target. Internal declarations are not public
`std` surface, require explicit internal-use acknowledgement outside their intended scope, and do not change the
semantics specified here.

The compiler does not recognize these modules by spelling. Their declarations remain ordinary standard-library
declarations unless another language rule explicitly identifies a particular declaration as compiler-known or
recognized.

## Public Declaration Surface

The declarations in this section are the public contract. The bodyless forms describe declaration surfaces and are not
source syntax that an ordinary package can use to omit a body. Fields named `internal state` represent private standard
library storage and are not part of the public package interface.

Changes to these modules must preserve the names, parameter modes, result shapes, ownership behavior, and blocking
requirements defined here unless the standard-library contract itself is revised.

Standard input, standard output, and standard error each own one operation-scoped synchronization guard. Input reads
from independently obtained values coordinate through the process-root input owner. `print` and `print_line` hold
one guard across all partial writes and their flush. Direct standard-stream writer operations acquire one guard per
call. Native leaves, generic writers, formatting, and buffering add no hidden guard. Captured, discarded, and inherited
destinations implement the same operation boundary independently, and lifecycle cleanup releases the guard on every
terminal path. Files and process pipes rely on their owned exclusive access rather than the standard-stream guard.

The `Arguments` sequence excludes the executable path. `Environment.entry_at` uses lexicographic unsigned target-native
key-code-unit order.

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
    mut async func read_async(pos destination: &mut [u8]) -> Result<usize, IoError>
        requires(blocking_execution());
}

trait AsyncWriter
{
    mut async func write_async(pos source: &[u8]) -> Result<usize, IoError>
        requires(blocking_execution());

    mut async func flush_async() -> Result<unit, IoError>
        requires(blocking_execution());
}

union SeekOrigin
{
    Start;
    Current;
    End;
}

trait Seeker
{
    mut func seek(offset: i64, origin: SeekOrigin) -> Result<u64, IoError>
        requires(blocking_execution());
}

trait AsyncSeeker
{
    mut async func seek_async(offset: i64, origin: SeekOrigin) -> Result<u64, IoError>
        requires(blocking_execution());
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

    construct(
        pos source: Source,
        capacity: usize,
    ) -> Result<Self, IoError>
        requires(capacity > 0);
}

struct BufferedWriter<Sink>
{
    internal state: BufferedWriterState<Sink>;

    construct(
        pos sink: Sink,
        capacity: usize,
    ) -> Result<Self, IoError>
        requires(capacity > 0);
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

func print(pos text: string) -> Result<unit, IoError>
    requires(blocking_execution());

func print_line(pos text: string) -> Result<unit, IoError>
    requires(blocking_execution());

async func print_async(pos text: string) -> Result<unit, IoError>
    requires(blocking_execution());

async func print_line_async(pos text: string) -> Result<unit, IoError>
    requires(blocking_execution());
```

`StandardInput` implements `Reader` and `AsyncReader`. `StandardOutput` and `StandardError` implement `Writer` and
`AsyncWriter`. These wrappers borrow product-lifetime standard streams and cannot close them. Buffered owners implement
the matching traits of their owned source or sink. Consuming a buffered writer through `into_sink` flushes it before
returning its sink.

`IoError.transferred` is the number of bytes committed before the reported failure. It is zero for operations without
byte transfer. A caller must not retry the already transferred prefix.

### `std.path` and `std.fs`

The native text and path declarations belong to `std.path`. The remaining declarations belong to `std.fs` and therefore
qualify path types through `std.path`.

```bray
union PathError
{
    InvalidNativeValue;
    NotUtf8;
    MemoryLayout;
}

struct NativeText
{
    internal state: NativeTextState;
}

impl NativeText
{
    construct from_string(pos value: string) -> Result<Self, PathError>;
    func to_string() -> Result<string, PathError>;
}

struct Path
{
    internal state: PathState;
}

impl Path
{
    construct from_native(pos value: NativeText) -> Result<Self, PathError>;
    construct from_string(pos value: string) -> Result<Self, PathError>;
    func to_native() -> &NativeText;
    func to_string() -> Result<string, PathError>;
    func join(pos child: &Path) -> Result<Path, PathError>;
    func normalize() -> Result<Path, PathError>;
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
    construct open(
        pos path: &std.path.Path,
        options: OpenOptions,
    ) -> Result<Self, std.io.IoError>
        requires(blocking_execution());

    func metadata() -> Result<FileMetadata, std.io.IoError>
        requires(blocking_execution());

    async func metadata_async() -> Result<FileMetadata, std.io.IoError>
        requires(blocking_execution());

    mut func read(pos destination: &mut [u8]) -> Result<usize, std.io.IoError>
        requires(blocking_execution());

    mut func write(pos source: &[u8]) -> Result<usize, std.io.IoError>
        requires(blocking_execution());

    mut func flush() -> Result<unit, std.io.IoError>
        requires(blocking_execution());

    mut func seek(offset: i64, origin: std.io.SeekOrigin) -> Result<u64, std.io.IoError>
        requires(blocking_execution());

    mut async func read_async(pos destination: &mut [u8]) -> Result<usize, std.io.IoError>
        requires(blocking_execution());

    mut async func write_async(pos source: &[u8]) -> Result<usize, std.io.IoError>
        requires(blocking_execution());

    mut async func flush_async() -> Result<unit, std.io.IoError>
        requires(blocking_execution());

    mut async func seek_async(offset: i64, origin: std.io.SeekOrigin) -> Result<u64, std.io.IoError>
        requires(blocking_execution());

    consume mut func close() -> Result<unit, std.io.IoError>
        requires(blocking_execution());

    consume mut async func close_async() -> Result<unit, std.io.IoError>
        requires(blocking_execution());
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
    construct open(pos path: &std.path.Path) -> Result<Self, std.io.IoError>
        requires(blocking_execution());

    mut func next() -> Result<DirectoryEntry?, std.io.IoError>
        requires(blocking_execution());

    mut async func next_async() -> Result<DirectoryEntry?, std.io.IoError>
        requires(blocking_execution());

    consume mut func close() -> Result<unit, std.io.IoError>
        requires(blocking_execution());

    consume mut async func close_async() -> Result<unit, std.io.IoError>
        requires(blocking_execution());
}

func metadata(pos path: &std.path.Path) -> Result<FileMetadata, std.io.IoError>
    requires(blocking_execution());

async func metadata_async(pos path: &std.path.Path) -> Result<FileMetadata, std.io.IoError>
    requires(blocking_execution());

func create_directory(pos path: &std.path.Path) -> Result<unit, std.io.IoError>
    requires(blocking_execution());

async func create_directory_async(pos path: &std.path.Path) -> Result<unit, std.io.IoError>
    requires(blocking_execution());

func remove_file(pos path: &std.path.Path) -> Result<unit, std.io.IoError>
    requires(blocking_execution());

async func remove_file_async(pos path: &std.path.Path) -> Result<unit, std.io.IoError>
    requires(blocking_execution());

func remove_directory(pos path: &std.path.Path) -> Result<unit, std.io.IoError>
    requires(blocking_execution());

async func remove_directory_async(pos path: &std.path.Path) -> Result<unit, std.io.IoError>
    requires(blocking_execution());

func rename(pos source: &std.path.Path, pos destination: &std.path.Path) -> Result<unit, std.io.IoError>
    requires(blocking_execution());

async func rename_async(
    pos source: &std.path.Path,
    pos destination: &std.path.Path,
) -> Result<unit, std.io.IoError>
    requires(blocking_execution());
```

`File` implements the blocking and asynchronous reader, writer, and seeker traits. A call disallowed by the file's
`OpenOptions` returns an `IoError` whose `kind` is `IoErrorKind.InvalidInput`. `Directory.next` and `next_async` return
`Result.Ok(none)` only after all entries have been observed.

Every asynchronous filesystem declaration has the same result and ownership effect as its blocking counterpart. A target
can implement it with a native completion source or a selected blocking lane, but that choice is not observable through
the signature. Every consuming `close` resolves its owner on both result variants. A close failure reports the failure
but does not return a live owner.

### `std.process`

```bray
struct Id
{
    internal state: ProcessIdState;
}

struct Arguments
{
    internal state: ArgumentsState;

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

    consume func close() -> Result<unit, std.io.IoError>
        requires(blocking_execution());
}

struct ChildOutput
{
    internal state: ChildOutputState;

    consume func close() -> Result<unit, std.io.IoError>
        requires(blocking_execution());
}

struct ChildCommand
{
    internal state: ChildCommandState;

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

struct ChildProcess
{
    internal state: ChildProcessState;

    func id() -> Id;

    mut func take_standard_input() -> ChildInput?;
    mut func take_standard_output() -> ChildOutput?;
    mut func take_standard_error() -> ChildOutput?;

    mut func request_termination() -> Result<unit, ChildError>
        requires(blocking_execution());

    consume func wait() -> Result<ExitStatus, ChildError>
        requires(blocking_execution());

    consume func force_termination() -> Result<ExitStatus, ChildError>
        requires(blocking_execution());
}
```

`ChildInput` implements `Writer`. `ChildOutput` implements `Reader`. A piped handle can be taken at most once. Inherited
and null policies produce no public pipe owner. Their consuming `close` operations resolve the pipe owner on both result
variants.

`request_termination` requests the target's cooperative termination mechanism without consuming the owner, waiting, or
reaping. Every returning `ChildProcess.wait` or `force_termination` path has reaped the child and resolved the owner,
including `Result.Error`. An infrastructure failure detected before reaping is retained while cleanup continues and is
returned only after ownership has been resolved. Normal scope exit requires an explicit consuming completion operation.

The typed `Process<T>` declarations specified by the concurrency chapter remain separate from `ChildProcess`. They use
`ChildProcess` internally and add the authenticated Bray protocol, asynchronous transport and completion,
cancellation-safe forced termination and reaping, and the `RunResult<T>` contract. The raw child and pipe owners remain
blocking-only so their APIs do not claim asynchronous behavior that merely blocks a cooperative worker.

### `std.time`

`std.time` provides exact durations, process-local monotonic instants, absolute timestamps, validated civil dates and
local date-times, fixed UTC offsets, named IANA time zones, zoned date-times, and calendar periods. It exposes
local-time conversion as unique, ambiguous, or nonexistent rather than silently selecting one side of a timezone
transition.

The complete public model, arithmetic rules, parsing and formatting contracts, and native-provider architecture are
defined by [Time library](time.md).

### `std.random`

```bray
union EntropyError
{
    Unavailable;
    Platform(pos error: std.io.IoError);
}

func fill_entropy(pos destination: &mut [u8]) -> Result<unit, EntropyError>
    requires(blocking_execution());

async func fill_entropy_async(pos destination: &mut [u8]) -> Result<unit, EntropyError>
    requires(blocking_execution());

struct Generator
{
    internal state: GeneratorState;
}

impl Generator
{
    construct(pos seed: [u8; 32]) -> Self;

    static func from_entropy() -> Result<Generator, EntropyError>
        requires(blocking_execution());

    static async func from_entropy_async() -> Result<Generator, EntropyError>
        requires(blocking_execution());

    mut func fill(pos destination: &mut [u8]);
    mut func next_u64() -> u64;
}
```

`fill_entropy` and `fill_entropy_async` either initialize the entire destination or return an error whose nested
`IoError.transferred` states the initialized prefix. `Generator` uses xoshiro256**. Its 32-byte seed is decoded as four
little-endian `u64` state words. The all-zero state is replaced with state words `[11400714819323198485, 0, 0, 0]`.
`fill` emits each `next_u64` result in little-endian byte order. Equal seeds therefore produce equal byte and `u64`
sequences on every target. No generator method consults system entropy after construction.

The async entropy operations defer `blocking_execution()` into their futures. Starting one selects a compatible blocking
lane. direct await requires the current lane to permit blocking. This keeps operating-system entropy acquisition off
cooperative workers without duplicating the platform entropy provider.

## Navigation

- [I/O and platform language semantics](../language/io-and-platform-services.md)
- [I/O and platform implementation architecture](io-and-platform-services.md)
- [Time library design](time.md)
- [Standard library design](standard-library.md)
