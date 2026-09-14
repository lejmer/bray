use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

#[test]
fn alternating_checkouts_reuse_only_their_own_intermediates() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let first = directory.path().join("checkout a");
    let second = directory.path().join("checkout b");
    let target = directory.path().join("shared target");

    create_workspace(&first, "a", "bool", "true", "answer()")?;
    create_workspace(&second, "b", "Result<(), ()>", "Ok(())", "answer().is_ok()")?;

    for profile in ["dev", "release"] {
        for (workspace, expected, fresh) in [
            (&first, "a", false),
            (&second, "b", false),
            (&first, "a", true),
            (&second, "b", true),
        ] {
            let mut command = Command::new("cargo");

            command
                .current_dir(if fresh {
                    workspace.join("app")
                } else {
                    workspace.to_path_buf()
                })
                .env_remove("CARGO_BUILD_BUILD_DIR")
                .env("CARGO_TARGET_DIR", &target)
                .args([
                    "build",
                    "--offline",
                    "--message-format=json",
                    "--profile",
                    profile,
                ]);

            if fresh {
                command.arg("--target-dir").arg(&target);
            }

            let output = run(&mut command)?;

            let records = String::from_utf8(output.stdout)?
                .lines()
                .map(serde_json::from_str::<Value>)
                .collect::<Result<Vec<_>, _>>()?;

            let artifacts = records
                .iter()
                .filter(|record| record["reason"] == "compiler-artifact")
                .collect::<Vec<_>>();

            assert_eq!(artifacts.len(), 3, "library, build script, executable");

            assert!(
                artifacts.iter().all(|record| record["fresh"] == fresh),
                "{records:#?}"
            );

            let executable = target
                .join(if profile == "dev" { "debug" } else { profile })
                .join(format!("storage-probe{}", std::env::consts::EXE_SUFFIX));

            let actual = run(&mut Command::new(executable))?;

            assert_eq!(String::from_utf8(actual.stdout)?.trim(), expected);
        }
    }

    Ok(())
}

fn create_workspace(
    root: &Path,
    tag: &str,
    return_type: &str,
    value: &str,
    assertion: &str,
) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(root.join(".cargo"))?;
    fs::create_dir_all(root.join("app/src"))?;
    fs::create_dir_all(root.join("dependency/src"))?;

    fs::write(
        root.join(".cargo/config.toml"),
        include_str!("../../.cargo/config.toml"),
    )?;

    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"app\", \"dependency\"]\nresolver = \"3\"\n",
    )?;

    fs::write(
        root.join("app/Cargo.toml"),
        "[package]\nname = \"storage-probe\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[dependencies]\nprobe-dependency = { path = \"../dependency\" }\n",
    )?;

    fs::write(
        root.join("dependency/Cargo.toml"),
        "[package]\nname = \"probe-dependency\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    )?;

    fs::write(
        root.join("dependency/src/lib.rs"),
        "mod value;\npub use value::answer;\n",
    )?;

    fs::write(
        root.join("dependency/src/value.rs"),
        format!("pub fn answer() -> {return_type} {{ {value} }}\n"),
    )?;

    fs::write(root.join("app/tag.txt"), tag)?;

    fs::write(
        root.join("app/build.rs"),
        r#"fn main() {
    println!("cargo::rerun-if-changed=tag.txt");
    let tag = std::fs::read_to_string("tag.txt").unwrap();
    println!("cargo::rustc-env=PROBE_TAG={tag}");
}
"#,
    )?;

    fs::write(
        root.join("app/src/main.rs"),
        format!(
            "use probe_dependency::answer;\nfn main() {{ assert!({assertion}); println!(\"{{}}\", env!(\"PROBE_TAG\")); }}\n"
        ),
    )?;

    Ok(())
}

fn run(command: &mut Command) -> Result<Output, Box<dyn Error>> {
    let output = command.output()?;

    assert!(
        output.status.success(),
        "{command:?}: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(output)
}
