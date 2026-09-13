# Time library

`std.time` separates elapsed time, absolute instants, civil calendar values, and timezone interpretation. These are
separate models because their identities and arithmetic answer different questions. Public language behavior belongs in
[I/O and platform services](../language/io-and-platform-services.md).

## Value and policy boundaries

Durations describe exact elapsed spans. Monotonic readings belong to a process-local clock identity. Timestamps identify
absolute instants independently of calendars. Civil dates and times acquire an absolute interpretation only through an
offset or timezone. Calendar periods remain distinct from elapsed durations.

Named zones are immutable descriptions backed by a known database version. Zoned values retain the timestamp as the
instant's identity and derive civil fields from zone rules. Public identities remain meaningful across serialization.
Process-local provider handles do not escape into package interfaces or serialized values.

The API makes policy choices explicit. Validation does not silently normalize invalid calendar input, and conversion
exposes ambiguity or gaps rather than guessing the caller's intended instant. Parsing and formatting remain strict and
locale-independent at this layer. Localized presentation belongs to locale services.

## Native provider

A pinned revision of Howard Hinnant's `date` and `tz` libraries supplies calendar and timezone algorithms. The toolchain
also pins IANA timezone data and CLDR Windows-to-IANA mappings. Their provenance is part of the selected toolchain, so
host libraries and runtime downloads cannot change time behavior.

Demanded provider code and timezone data link from toolchain-supplied static support. Products require no separately
installed provider DLL, C++ runtime package, or timezone database.

Bray owns the public types, ownership, failures, and policy vocabulary. A small private C ABI exchanges fixed-width
values, caller-owned buffers, and opaque local identities. C++ layouts, allocators, and exceptions remain behind that
boundary. Failures cross it as closed status values. Host-zone discovery maps native identities into the pinned IANA
namespace and can report that no deterministic mapping is available.

## Demand and retention

Monotonic clocks, civil calendar operations, parsing and formatting, and named-zone services have distinct retention
boundaries. The timezone database belongs to named-zone support. Selecting ordinary filesystem, process, or clock
services must not pull the temporal provider into a product.

Provider code and immutable data are independently discardable across supported native object formats. Target artifact
metadata records provider inputs and capability partitions, while public package interfaces contain ordinary Bray
declarations. This keeps third-party names and physical database locations out of the public semantic model.

Timezone loading is demand-driven and thread-safe. Immutable provider state may be interned by stable zone name and
pinned database version, including failed lookups. Caching changes cost without changing interpretation.

Blocking time operations follow the execution-lane model shared with other platform services. An asynchronous wrapper
carries its deferred requirements so starting it chooses a compatible lane and direct awaiting checks the current one.

## Related documents

- [I/O and platform service design](io-and-platform-services.md)
- [Standard library](standard-library.md)
- [Async runtime](async-runtime.md)
