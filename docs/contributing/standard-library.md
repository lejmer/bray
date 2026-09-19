# Build and check the standard library

Use these workflows after changing `standard-library/` or preparing an installed toolchain. For native tests, provision
[LLVM](llvm.md) and configure a native linker. Follow the [artifact storage rules](build-artifacts.md) across checkouts.

## Build a bundle

Build the standard-library bundle from the checked-in `standard-library/` workspace:

```text
cargo xtask standard-library build --output <directory>
```

Use `--source <directory>` to select another standard-library workspace. Without `--target`, the bundle contains
artifacts for every supported native target. Use `--target <triple>` to build one target. A successful rebuild
transactionally replaces an existing bundle at the output path.

The bundle records the exact source, dependency, compiler, and packaging inputs that produced it. Repeated builds
authenticate the manifest and every published artifact, then reuse a current bundle that contains all requested targets.

To produce the installed layout expected by Bray Tack beside release binaries in `target/release/`, run:

```text
cargo xtask standard-library build --output target/release/lib/bray/standard-library
```

## Run focused native tests

Start with the small [native composition corpus](testing.md#native-composition-tests), then run native standard-library
integration tests with:

```text
cargo xtask standard-library test
```

Use one or more `--part` options to run only the needed phases. Available parts are `provider-retention`,
`interoperability`, `api`, and `outcomes`. Omitting `--part` runs every phase. For example, run only the public API
tests with:

```text
cargo xtask standard-library test --part api
```

Native compiler workflows build and use optimized `bray` and `brayc` tools. To retain one bounded compiler trace for
every build invocation under a distinct phase directory, run:

```text
cargo xtask standard-library test --profile-output profiles/standard-library-native
```

The profiling form preserves the same test contract and streams Bray build progress while it runs. Inspect the resulting
reports with the profile commands described in [Compiler profiling](profiling.md).

## Verify conformance and reproducibility

```text
cargo xtask standard-library verify
```

This checks standard-library conformance and reproducible bundle production. It also checks generated OS-binding bytes
and Unicode data, and source-checks the generated Bray declarations for every supported native target. Follow the [OS
binding workflow](os-bindings.md) when changing platform declarations or SDK baselines.

## Regenerate Unicode tables

After changing the pinned Unicode inputs or their generator, regenerate the character-property tables and metadata:

```text
cargo xtask standard-library unicode generate
```

The inputs live under `standard-library/targets/unicode/17.0.0`. The command writes
`standard-library/std/src/runtime/character/unicode_tables.bray` and `standard-library/targets/unicode/metadata.json`.
To check that both generated files are current without changing them, run:

```text
cargo xtask standard-library unicode generate --check
```

`standard-library verify` includes this freshness check. Regenerate stale files before building a bundle, since bundle
builds also check the generated Unicode data.

## Diagnose a slow build

Follow [standard-library compiler profiling](profiling.md#profile-a-standard-library-build) to collect per-target
reports. Profiled builds bypass bundle reuse so the compiler work is measured on every run.

## Understand bundle contents

The target bundle records the Bray standard-library archive, its direct native link requirements, and the separate
third-party temporal provider. A target that declares temporal roles must declare the complete temporal role inventory
or the build fails.

Return to [repository tasks](xtask.md) to choose another workflow.
