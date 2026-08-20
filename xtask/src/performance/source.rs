pub(super) const STREAM_OUTPUT: &str = r#"module stream_output;

using std.io;

func main() -> Result<unit, std.io.IoError>
{
    let mut index: usize = 0;

    while index < 1024
    {
        try std.io.print(&"x");
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
        try await std.io.print_async(&"x");
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
        try await std.io.print_async(&"x");
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
