# LLVM Toolchain

Bray's first code generation backend targets LLVM 22.1.8. Compiler development, continuous integration, and release builds must use
the exact host package recorded in `toolchains/llvm.json`.

See [Repository tasks](xtask.md) for the complete development-command reference.

Provision the supported package for the active Rust host:

```text
cargo llvm fetch
```

The LLVM command is dependency-light, so it remains available before LLVM-dependent compiler crates can be built.

The command downloads the official LLVM development archive with `curl`, verifies its exact byte length and SHA-256 digest, extracts
it with `tar`, and validates the LLVM version, C headers, and static core and target libraries. Provisioned files remain under
Cargo's ignored `target/toolchains/llvm` directory. Repeated commands reuse a valid archive and installation.

Cargo receives the provisioned prefix through `BRAY_LLVM_PREFIX` in `.cargo/config.toml`. The configuration also supplies the
versioned environment name consumed by the upstream `llvm-sys` build script, but Bray-owned code does not use that external crate's
name. Set both variables before invoking Cargo to use a separately managed LLVM installation. Validate an external installation
before building:

```text
cargo llvm validate --root <directory>
```

An external installation must provide the exact pinned version and the same development surface. The build does not search `PATH`
for another LLVM installation.

The supported Rust host triples are listed in `toolchains/llvm.json`. Adding another host requires an official LLVM development
archive, its exact release metadata, and CI coverage for provisioning and backend compilation on that host.
