pub(super) const STREAM_OUTPUT: &str = r#"module stream_output;

using std.io;

func main() -> Result<unit, std.io.IoError>
{
    let mut index: usize = 0;

    while index < 1024
    {
        try std.io.print("x");
        index += 1;
    }

    return Ok(unit);
}
"#;

pub(super) const ASYNC_OUTPUT: &str = r#"module async_output;

using std.io;

async func main() -> Result<unit, std.io.IoError>
{
    let mut index: usize = 0;

    while index < 128
    {
        try await std.io.print_async("x");
        index += 1;
    }

    return Ok(unit);
}
"#;

pub(super) const CONTENDED_OUTPUT: &str = r#"module contended_output;

using std.io;

async func write_output() -> Result<unit, std.io.IoError>
{
    let mut index: usize = 0;

    while index < 64
    {
        try await std.io.print_async("x");
        index += 1;
    }

    return Ok(unit);
}

async func main() -> Result<unit, std.io.IoError>
{
    let first: Task<Result<unit, std.io.IoError>> = write_output().start();
    let second: Task<Result<unit, std.io.IoError>> = write_output().start();

    match try await first.join()
    {
        case Ok(_) {}
        case Error(error) { return Error(error); }
    }

    match try await second.join()
    {
        case Ok(_) {}
        case Error(error) { return Error(error); }
    }

    return Ok(unit);
}
"#;

pub(super) const FILE_OUTPUT: &str = r#"module file_output;

using std.fs;
using std.fs.FileWriter;
using std.io;
using std.io.BufferedWriterSinkWriter;
using std.path;
using std.testing;

func main() -> Result<unit, std.io.IoError>
    requires(blocking_execution())
{
    let path: std.path.Path = output_path();

    match std.fs.remove_file(&path)
    {
        case Ok(_) {}
        case Error(_) {}
    }

    let options: std.fs.OpenOptions = std.fs.OpenOptions(
        access = std.fs.FileAccess.Write,
        creation = std.fs.FileCreation.CreateNew,
    );

    let opened: std.fs.File = std.testing.assert_ok<std.fs.File, std.io.IoError>(
        std.fs.File.open(&path, options = options),
    );
    let mut writer: std.io.BufferedWriter<std.fs.File> =
        std.testing.assert_ok<std.io.BufferedWriter<std.fs.File>, std.io.IoError>(
            std.io.BufferedWriter<std.fs.File>(opened, capacity = 256),
        );
    let bytes: [u8; 4096] = [120; 4096];

    std.testing.assert_ok<unit, std.io.IoError>(writer.write_all(&bytes[..]));

    let mut completed: std.fs.File = std.testing.assert_ok<std.fs.File, std.io.IoError>(writer.into_sink());
    std.testing.assert_ok<unit, std.io.IoError>(completed.close());
    assert(completed.is_complete());
    try std.fs.remove_file(&path);

    return Ok(unit);
}

func output_path() -> std.path.Path
{
    match std.path.Path.from_string(&"bray-performance-file-output")
    {
        case Ok(path) { return path; }
        case Error(_) { panic("performance output path must be valid"); }
    }
}
"#;

pub(super) const CAPTURED_OUTPUT: &str = r#"module captured_output;

using core.target;
using std.bytes;
using std.io;
using std.memory;

struct CapturedOutput
{
    mut bytes: std.bytes.Buffer;
}

impl CapturedOutputWriter = CapturedOutput(std.io.Writer)
{
    mut func write(pos source: &[u8]) -> Result<usize, std.io.IoError>
        requires(blocking_execution())
    {
        match trusted std.bytes.append(buffer = &mut self.bytes, bytes = source)
        {
            case Ok(_) { return Ok(source.length()); }
            case Error(_) { panic("captured output storage must remain available"); }
        }
    }

    mut func flush() -> Result<unit, std.io.IoError>
        requires(blocking_execution())
    {
        return Ok(unit);
    }
}

func main() -> Result<unit, std.io.IoError>
    requires(blocking_execution())
{
    let storage: std.bytes.Buffer = match trusted std.bytes.Buffer(capacity = 4096)
    {
        case Ok(buffer) { yield buffer; }
        case Error(_) { panic("captured output storage must be created"); }
    };

    let mut output: CapturedOutput =
    {
        bytes = storage,
    };

    let bytes: [u8; 4096] = [120; 4096];

    trusted retain_bytes(&bytes[..]);
    try output.write_all(&bytes[..]);
    trusted retain_bytes(output.bytes.as_slice());
    assert(std.bytes.length(&output.bytes) == 4096);

    return Ok(unit);
}

trusted func retain_bytes(pos bytes: &[u8])
    uses(device_memory, intrinsic, raw_memory, unchecked_alias, unchecked_init)
{
    let pointer: DataPointer<u8> = trusted internal std.memory.slice_pointer<u8>(bytes, count = bytes.length());

    trusted core.target.assembly<(DataPointer<u8>, usize), unit>(
        template = "",
        constraints = "reg,reg",
        clobbers = "memory",
        features = "",
        options = 0,
        inputs = (pointer, bytes.length()),
    );
}
"#;

pub(super) const PROCESS_PIPE_TRANSFER: &str = r#"module process_pipe_transfer;

using std.io;
using internal std.io.StandardInputReader;
using std.path;
using std.process;
using internal std.process.ChildInputWriter;
using std.testing;

func main() -> Result<unit, std.io.IoError>
    requires(blocking_execution())
{
    let arguments: std.process.Arguments = std.process.arguments();

    if arguments.length() > 0
    {
        return run_pipe_child();
    }

    return run_pipe_parent();
}

func run_pipe_child() -> Result<unit, std.io.IoError>
    requires(blocking_execution())
{
    let mut input: std.io.StandardInput = std.io.standard_input();
    let mut bytes: [u8; 4096] = [0; 4096];

    try input.read_exact(&mut bytes[..]);

    return Ok(unit);
}

func run_pipe_parent() -> Result<unit, std.io.IoError>
    requires(blocking_execution())
{
    let executable_key: std.path.NativeText = match consume std.path.NativeText.from_string(
        &"BRAY_PERFORMANCE_EXECUTABLE"
    )
    {
        case Ok(value) { yield value; }
        case Error(_) { panic("performance executable key must be valid"); }
    };

    let current_executable: std.path.NativeText? = match consume std.process.current_environment_value(&executable_key)
    {
        case Ok(value) { yield value; }
        case Error(_) { panic("performance executable path must be readable"); }
    };

    let executable_text: std.path.NativeText = match consume current_executable
    {
        case ?value { yield value; }
        case none { panic("performance executable path must be present"); }
    };

    let executable: std.path.Path = match consume std.path.Path.from_native(executable_text)
    {
        case Ok(path) { yield path; }
        case Error(_) { panic("performance executable path must be valid"); }
    };

    let mut command: std.process.ChildCommand = match consume std.process.ChildCommand(executable)
    {
        case Ok(created) { yield created; }
        case Error(_) { panic("performance child command must be created"); }
    };

    match command.environment_policy(policy = std.process.EnvironmentPolicy.Empty)
    {
        case Ok(_) {}
        case Error(_) { panic("performance child environment must be isolated"); }
    }

    let child_argument: std.path.NativeText = match consume std.path.NativeText.from_string(&"pipe-child")
    {
        case Ok(value) { yield value; }
        case Error(_) { panic("performance child argument must be valid"); }
    };

    match command.argument(child_argument)
    {
        case Ok(_) {}
        case Error(_) { panic("performance child argument must be added"); }
    }

    command.standard_input(policy = std.process.ChildStreamPolicy.Piped);
    command.standard_output(policy = std.process.ChildStreamPolicy.Null);
    command.standard_error(policy = std.process.ChildStreamPolicy.Null);

    let mut child: std.process.ChildProcess = match consume command.spawn()
    {
        case Ok(created) { yield created; }
        case Error(_) { panic("performance child must be started"); }
    };

    {
        let mut input: std.process.ChildInput = match consume child.take_standard_input()
        {
            case ?stream { yield stream; }
            case none { panic("performance child input must be piped"); }
        };

        let bytes: [u8; 4096] = [120; 4096];

        std.testing.assert_ok<unit, std.io.IoError>(input.write_all(&bytes[..]));
        std.testing.assert_ok<unit, std.io.IoError>(input(std.io.Writer).flush());
        std.testing.assert_ok<unit, std.io.IoError>(input.close());
        assert(input.is_complete());
    }

    let status: std.process.ExitStatus = match consume child.wait()
    {
        case Ok(value) { yield value; }
        case Error(_) { panic("performance child must be joined"); }
    };

    assert(child.is_complete());

    match status
    {
        case std.process.ExitStatus.Code(0) {}
        case _ { panic("performance child must exit successfully"); }
    }

    return Ok(unit);
}
"#;

pub(super) const FILESYSTEM_DIRECTORY_ITERATION: &str = r#"module filesystem_directory_iteration;

using std.fs;
using std.io;
using std.path;
using std.testing;

func path(pos text: string) -> std.path.Path
{
    return std.testing.assert_ok<std.path.Path, std.path.PathError>(std.path.Path.from_string(&text));
}

func create_file(pos name: &std.path.Path) -> Result<unit, std.io.IoError>
    requires(blocking_execution())
{
    let options: std.fs.OpenOptions = std.fs.OpenOptions(
        access = std.fs.FileAccess.Write, creation = std.fs.FileCreation.CreateNew,
    );
    let mut file: std.fs.File = try std.fs.File.open(name, options = options);
    std.testing.assert_ok<unit, std.io.IoError>(file.close());
    assert(file.is_complete());
    return Ok(unit);
}

trusted func main() -> Result<unit, std.io.IoError>
    requires(blocking_execution())
{
    let directory: std.path.Path = path("bray-performance-listing");
    let a: std.path.Path = path("bray-performance-listing/a");
    let b: std.path.Path = path("bray-performance-listing/b");
    let c: std.path.Path = path("bray-performance-listing/c");
    let d: std.path.Path = path("bray-performance-listing/d");
    let e: std.path.Path = path("bray-performance-listing/e");
    let f: std.path.Path = path("bray-performance-listing/f");
    let g: std.path.Path = path("bray-performance-listing/g");
    let h: std.path.Path = path("bray-performance-listing/h");
    try std.fs.create_directory(&directory);
    try create_file(&a);
    try create_file(&b);
    try create_file(&c);
    try create_file(&d);
    try create_file(&e);
    try create_file(&f);
    try create_file(&g);
    try create_file(&h);
    let mut rounds: usize = 0;
    let mut count: usize = 0;
    while rounds < 128
    {
        let mut listing: std.fs.Directory = try trusted std.fs.Directory.open(&directory);
        loop
        {
            let next: std.fs.DirectoryEntry? = std.testing.assert_ok<std.fs.DirectoryEntry?, std.io.IoError>(
                trusted listing.next()
            );
            match consume next
            {
                case ?entry { assert(entry.metadata.kind matches std.fs.FileKind.File); count += 1; }
                case none { break; }
            }
        }
        std.testing.assert_ok<unit, std.io.IoError>(listing.close());
        assert(listing.is_complete());
        rounds += 1;
    }
    assert(count == 1024);
    try std.fs.remove_file(&a);
    try std.fs.remove_file(&b);
    try std.fs.remove_file(&c);
    try std.fs.remove_file(&d);
    try std.fs.remove_file(&e);
    try std.fs.remove_file(&f);
    try std.fs.remove_file(&g);
    try std.fs.remove_file(&h);
    try std.fs.remove_directory(&directory);
    return Ok(unit);
}
"#;
