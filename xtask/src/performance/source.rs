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

func main() -> Result<unit, std.io.IoError>
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

    let opened: std.fs.File = try std.fs.File.open(&path, options = options);
    let mut writer: std.io.BufferedWriter<std.fs.File> =
        try std.io.BufferedWriter<std.fs.File>(opened, capacity = 256);
    let bytes: [u8; 4096] = [120; 4096];

    try std.io.write_all<std.io.BufferedWriter<std.fs.File>>(&mut writer, &bytes[..]);

    let mut completed: std.fs.File = try writer.into_sink();
    try completed.close();
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

using std.bytes;
using std.io;

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

    try std.io.write_all<CapturedOutput>(&mut output, &bytes[..]);
    assert(std.bytes.length(&output.bytes) == 4096);

    return Ok(unit);
}
"#;

pub(super) const PROCESS_PIPE_TRANSFER: &str = r#"module process_pipe_transfer;

using std.fs;
using std.io;
using internal std.io.StandardInputReader;
using std.path;
using std.process;
using internal std.process.ChildInputWriter;

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

    try std.io.read_exact<std.io.StandardInput>(&mut input, &mut bytes[..]);

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

    let environment: std.process.Environment = std.process.environment();

    let executable_text: std.path.NativeText = match consume environment.value(&executable_key)
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

    let observation_name: string = "bray-performance-pipe-child-observations";

    let observation_key: std.path.NativeText = match consume std.path.NativeText.from_string(
        &"BRAY_PERFORMANCE_OBSERVATION_PATH"
    )
    {
        case Ok(value) { yield value; }
        case Error(_) { panic("performance observation key must be valid"); }
    };

    let observation_value: std.path.NativeText = match consume std.path.NativeText.from_string(&observation_name)
    {
        case Ok(value) { yield value; }
        case Error(_) { panic("performance child observation path must be valid"); }
    };

    match command.set_environment(observation_key, observation_value)
    {
        case Ok(_) {}
        case Error(_) { panic("performance child observation path must be configured"); }
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

    let mut input: std.process.ChildInput = match consume child.take_standard_input()
    {
        case ?stream { yield stream; }
        case none { panic("performance child input must be piped"); }
    };

    let bytes: [u8; 4096] = [120; 4096];

    try std.io.write_all<std.process.ChildInput>(&mut input, &bytes[..]);
    try input(std.io.Writer).flush();
    try input.close();

    let status: std.process.ExitStatus = match consume child.wait()
    {
        case Ok(value) { yield value; }
        case Error(_) { panic("performance child must be joined"); }
    };

    match status
    {
        case std.process.ExitStatus.Code(0) {}
        case _ { panic("performance child must exit successfully"); }
    }

    let observation_path: std.path.Path = match consume std.path.Path.from_string(&observation_name)
    {
        case Ok(path) { yield path; }
        case Error(_) { panic("performance child observation path must be valid"); }
    };

    match std.fs.remove_file(&observation_path)
    {
        case Ok(_) {}
        case Error(_) {}
    }

    return Ok(unit);
}
"#;
