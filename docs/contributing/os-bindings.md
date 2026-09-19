# Generate and verify OS bindings

Use this workflow when changing target-specific operating-system declarations. Production compilation uses generated
Bray declarations, so contributors need the pinned SDK only when running a native probe.

## Regenerate declarations and probes

Regenerate the checked-in target-specific operating-system bindings and native probes from the pinned SDK description:

```text
cargo xtask standard-library os-bindings generate
```

The generator reads `standard-library/targets/os-bindings.json`, which lists focused sources beneath
`standard-library/targets/os-bindings`. Shared POSIX and operating-system files hold common declarations, while one file
per exact target names its pinned SDK authority, revision, header set, and target-specific declarations.
The generator writes Bray declarations beneath
`standard-library/std/src/os/generated` and matching C probes beneath `standard-library/targets/probes`. Every generated
file records the complete model's SHA-256 digest. Production compilation consumes only the generated Bray declarations
and does not inspect ambient host headers.

Check that the generated bindings, probes, and recorded input digest are current without modifying them:

```text
cargo xtask standard-library os-bindings generate --check
```

## Check against the pinned SDK

Compile and run the authoritative native probe for the current host or an explicitly named matching target with:

```text
cargo xtask standard-library os-bindings probe [--target <triple>] --sdk-root <path> [--compiler-root <path>]
```

The SDK root is the exact Linux sysroot, macOS SDK directory, or Windows Kits root named by the model. The probe removes
ambient SDK include and library paths, selects that root explicitly, and verifies the recorded SDK revision before it
checks emitted constants, scalar mappings, layouts, field offsets, flexible tails, bitfield access, callable signatures,
linked symbols, thread-local storage, and dynamic lookup. A probe runs only on its exact target host. For checks across all supported targets, use
[standard-library verification](standard-library.md#verify-conformance-and-reproducibility).

Windows probes also require `--compiler-root` naming the exact Visual C++ tools revision recorded by the model. This
provides the UCRT compiler headers and libraries without restoring ambient Visual Studio discovery.

## Choose an SDK baseline

SDK revisions are compatibility baselines, not rolling references to the newest release. Select a baseline from the
platform owner's published stable releases, record the exact authority and revision in the model, and retain it until a
deliberate compatibility review adopts a replacement. Linux baselines must identify a coherent kernel and libc sysroot.
The Linux baseline is the Debian 13 stable sysroot, whose release inventory pairs the Linux 6.12 LTS series with glibc
2.41 for both supported architectures. The choice is recorded against the
[Debian 13 release](https://www.debian.org/News/2025/20250809) and the active long-term status published by
[kernel.org](https://www.kernel.org/releases.html). Windows and macOS choices are recorded against the corresponding
[Microsoft SDK archive](https://learn.microsoft.com/windows/apps/windows-sdk/downloads) and
[Apple Xcode SDK table](https://developer.apple.com/xcode/system-requirements).

Return to [standard-library workflows](standard-library.md) or [repository tasks](xtask.md).
