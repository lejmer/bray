# Repository Tasks

Bray keeps repository-specific development automation in the `xtask` crate. Run these commands from the repository root with
`cargo xtask`. They are contributor and release-engineering tools rather than user-facing Bray Tack commands.

## Style

Apply deterministic formatting fixes and run the repository's structural style checks:

```text
cargo xtask style
```

Run the same checks without changing files:

```text
cargo xtask style check
```

The mutating command is required after changing Rust source. See [Coding conventions](coding-conventions.md) for the enforced
rules.

## Bray source formatting

Format maintained Bray source in the standard library, conformance suites, examples, readiness fixtures, and recovery corpus:

```text
cargo xtask format
```

Check the same source set without modifying it:

```text
cargo xtask format check
```

The general `style` commands include the corresponding Bray source formatting operation.

## Compiler-known catalog

Regenerate the Rust source and digest derived from the checked-in compiler-known catalog definitions:

```text
cargo xtask compiler-known generate
```

Check whether the generated files are current without changing them:

```text
cargo xtask compiler-known generate --check
```

Check the generated files and semantically validate the complete compiler-known catalog:

```text
cargo xtask compiler-known check
```

## LLVM toolchain

Download and validate the pinned LLVM toolchain for the active Rust host:

```text
cargo xtask llvm fetch
```

Validate the provisioned toolchain, or validate an explicitly managed installation:

```text
cargo xtask llvm validate
cargo xtask llvm validate --root <directory>
```

Print the supported Rust host identity selected by the LLVM manifest:

```text
cargo xtask llvm host
```

See [LLVM toolchain](llvm.md) for provisioning policy and supported hosts.

## Package interfaces

Render a package interface as JSON. Without section filters, inspection includes every section:

```text
cargo xtask package-interface inspect <path>
cargo xtask package-interface inspect <path> --section <name> [--section <name>]...
```

Validate the complete package interface and render its validated identity as JSON:

```text
cargo xtask package-interface validate <path>
```

Section names are the stable machine-readable names printed by the unfiltered inspection output.

## Readiness audits

Run all structural readiness audits:

```text
cargo xtask readiness
```

Run one focused audit:

```text
cargo xtask readiness semantic
cargo xtask readiness diagnostics
cargo xtask readiness lowering
cargo xtask readiness memory
cargo xtask readiness codegen
cargo xtask readiness emission
cargo xtask readiness linker
```

Compile, inspect, link, and execute the native readiness fixtures separately:

```text
cargo xtask readiness native-execution
```

The default readiness command does not include native execution. See [Testing](testing.md) for the coverage fixtures and audit
contracts.

## Runtime artifacts

Build the reference runtime archive and its metadata into the toolchain runtime bundle:

```text
cargo xtask runtime-artifact build --output <directory> [--target <triple>] [--profile <profile>]
```

The command creates a target-specific directory beneath the output root and transactionally replaces an existing artifact for
that target. The target defaults to the compiler host and the Cargo profile defaults to `release`. For example, produce the
installed host runtime layout expected beside release binaries with:

```text
cargo xtask runtime-artifact build --output target/release/lib/bray/runtime
```

Run the host runtime build and native-link smoke test with:

```text
cargo xtask runtime-artifact smoke-test
```

## Standard library

Build the standard-library bundle from the checked-in `standard-library/` workspace:

```text
cargo xtask standard-library build --output <directory>
```

Use `--source <directory>` to select another standard-library workspace. Without `--target`, the bundle contains artifacts for
every supported native target. Use `--target <triple>` to build one target. A successful rebuild transactionally replaces an
existing bundle at the output path.

To produce the installed layout expected by Bray Tack beside release binaries in `target/release/`, run:

```text
cargo xtask standard-library build --output target/release/lib/bray/standard-library
```

Run native standard-library conformance tests with:

```text
cargo xtask standard-library test
```

Verify standard-library conformance and reproducible bundle production with:

```text
cargo xtask standard-library verify
```
