# Time Library

This document defines the goal-state architecture and public model for `std.time`. The language semantics that users may
rely on are specified in [I/O and platform services](../language/io-and-platform-services.md). The private native
boundary follows [I/O and platform service architecture](io-and-platform-services.md).

## Goals

The time library must:

- distinguish elapsed time, absolute timestamps, local calendar values, UTC offsets, and named time zones,
- perform checked calendar and timestamp arithmetic without silent overflow or calendar normalization,
- use one pinned IANA timezone database for deterministic named-zone behavior across supported targets,
- expose ambiguous and nonexistent local times instead of silently selecting an answer,
- support strict standard timestamp interchange without requiring locale services,
- keep monotonic timing usable without linking calendar or timezone support,
- and avoid reimplementing calendar and timezone algorithms in Bray.

## Native Temporal Provider

Civil calendar and timezone behavior is provided by a pinned source revision of Howard Hinnant's `date` and `tz` C++
libraries. The toolchain also pins one IANA timezone database release and one CLDR Windows-to-IANA mapping release.
These inputs are content-addressed, recorded in toolchain provenance, and verified before use.

The provider is built into a static support archive. A product links its provider objects and timezone data only when it
demands named-zone services. Users do not install a DLL, shared object, C++ runtime package, or timezone database
separately. The provider never downloads data at runtime and never substitutes a host timezone database for the pinned
toolchain data.

No C++ type, exception, allocator, string, or object layout crosses the native boundary. A small C ABI exchanges
fixed-width scalars, validated caller-owned buffers, and process-local opaque timezone identities. All C++ exceptions
are caught before the boundary and converted into closed status values.

The provider owns these difficult and externally maintained rules:

- proleptic Gregorian calendar validation and arithmetic,
- IANA zone lookup, aliases, historical transitions, and future rules,
- absolute-to-local and local-to-absolute conversion,
- unique, repeated, and skipped local-time classification,
- UTC offset and timezone abbreviation lookup,
- and strict timestamp parsing and formatting primitives.

Host-local timezone discovery maps target-native zone identities into the pinned IANA namespace. Windows discovery uses
the pinned CLDR mapping rather than interpreting registry names as IANA names. A host identity with no deterministic
mapping produces `Unavailable`.

Bray owns the public types, failure values, ownership rules, formatting ergonomics, and policy choices. The public API
does not mirror the C++ API.

## Core Value Model

`Duration` is a signed exact span measured in seconds and nanoseconds. Its normalized representation uses floor seconds
and a nonnegative nanosecond remainder below one billion. Duration arithmetic is independent of calendars and time
zones.

`Instant` is a process-local monotonic reading. It can establish ordering, elapsed durations, and deadlines only within
its source clock identity. It is not serializable and cannot become a `Timestamp`.

`Timestamp` is an absolute instant represented relative to the Unix epoch with nanosecond precision. It has no calendar
or timezone until interpreted through UTC, a fixed `UtcOffset`, or a named `TimeZone`.

`Date` is a validated proleptic Gregorian year, month, and day. `TimeOfDay` is a validated hour, minute, second, and
nanosecond. `LocalDateTime` combines those values without claiming an absolute instant or timezone.

`UtcOffset` is a validated fixed displacement from UTC. It is not a named timezone and does not carry transition rules.

`TimeZone` is an immutable named IANA timezone. Its process-local provider identity is an implementation detail. Its
defined name and timezone-database version are observable. It is not serialized by its process-local identity.

`ZonedDateTime` combines a `Timestamp` and `TimeZone`. The timestamp remains the identity of the instant. Local calendar
fields, offset, daylight-saving state, and abbreviation are derived from the pinned timezone rules.

`CalendarPeriod` is a signed number of years, months, and days. It represents calendar-relative movement and is
deliberately not a `Duration`. Adding 24 hours and adding one calendar day may produce different timestamps across a
timezone transition.

## Public Surface

The public package provides these types:

```bray
struct Duration;
struct Instant;
struct Timestamp;
struct Date;
struct TimeOfDay;
struct LocalDateTime;
struct UtcOffset;
struct TimeZone;
struct ZonedDateTime;
struct CalendarPeriod;

union LocalTimeResolution
{
    Unique(pos value: ZonedDateTime);
    Ambiguous(pos earlier: ZonedDateTime, pos later: ZonedDateTime);
    Nonexistent(pos before: ZonedDateTime, pos after: ZonedDateTime);
}

union TimeError
{
    Unavailable;
    InvalidValue;
    UnknownTimeZone;
    OutOfRange;
    InvalidFormat(pos offset: usize);
    Platform(pos error: std.io.IoError);
}
```

Construction validates fields rather than normalizing invalid input. For example, February 30 is an error and never
becomes a March date implicitly. Checked arithmetic reports `OutOfRange`. Calendar operations that can encounter an
invalid destination day require an explicit `DateAdjustment` policy such as reject or clamp.

The principal operations are:

- `monotonic_now`, `wall_now`, blocking `sleep`, and blocking-lane `sleep_async`,
- checked duration and timestamp arithmetic,
- checked date and calendar-period arithmetic,
- UTC and fixed-offset conversion without named-zone data,
- `TimeZone.load` by IANA name and fallible discovery of the host's local zone,
- conversion from a timestamp to one unambiguous `ZonedDateTime`,
- conversion from a local date-time to `LocalTimeResolution`,
- strict RFC 3339 and ISO 8601 parsing and formatting,
- and explicit formatting patterns whose syntax is owned by Bray.

Conveniences may choose an ambiguous or nonexistent local-time result only when their names state the policy, such as
`earlier` or `later`. A general conversion does not make that decision for the caller.

`sleep_async` carries `blocking_execution()` as a deferred execution requirement. Starting its future selects a
compatible blocking lane, while directly awaiting it requires the current lane to permit blocking. The operation
therefore does not block a cooperative worker.

## Arithmetic

Exact arithmetic operates on `Duration`, `Instant`, and `Timestamp`. Adding a duration to a zoned value first changes
its timestamp and then derives new local fields from the zone.

Calendar arithmetic operates on `Date`, `LocalDateTime`, or the local representation of a `ZonedDateTime`. Applying a
`CalendarPeriod` to a zoned value can produce any `LocalTimeResolution` and therefore cannot return a bare
`ZonedDateTime` unless the caller supplies an explicit transition policy.

Leap seconds follow the behavior declared by the pinned provider and database. The selected policy and database version
are part of toolchain provenance. They cannot vary according to host libraries.

## Parsing And Formatting

`Timestamp` supports strict RFC 3339 interchange. `Date`, `TimeOfDay`, and `LocalDateTime` support their corresponding
ISO 8601 forms. Parsing consumes the complete input unless an explicitly named prefix parser is used. Failures report
the first invalid byte offset through `TimeError.InvalidFormat`.

Bray formatting patterns are validated values rather than unchecked host `strftime` strings. Locale-sensitive names and
alternate calendar presentation belong to locale services. Core time formatting remains deterministic and
locale-independent.

## Provider Identity And Caching

Named timezone loading is demand-driven and thread-safe. The runtime may intern successfully loaded immutable zones by
stable name. Repeated loads reuse provider state without changing observable behavior. A failed lookup is also cacheable
for one pinned database version.

Compiled package interfaces expose only ordinary public `std.time` declarations. They do not contain native provider
identities, timezone handles, C++ names, source paths, or the physical timezone database location.

Every standard-library target artifact set includes temporal-provider provenance. The record identifies the shared
provider source, the capability partition for each native translation unit, the timezone database, host-zone mapping
data, and verified content digests used to build that target. The stable partition inventory drives native provider
construction. It is toolchain metadata rather than a runtime dependency.

Civil-date operations, deterministic parsing and formatting, and named-timezone operations occupy separate native
retention partitions. The embedded timezone database belongs only to the named-timezone partition. Native provider
compilation emits independently discardable function and immutable-data contributions for COFF, ELF, and Mach-O so a
product retains only demanded provider capabilities and their data.

Temporal platform ABI exports occupy a linkable component separate from unrelated platform mechanisms. Selecting an
ordinary filesystem, stream, process, clock, or entropy service cannot introduce a reference to the temporal provider.
Linker-map conformance enters through the public platform ABI and verifies that a product with no temporal demand
retains no temporal provider partition.

## Testing

Conformance covers:

- leap years, month lengths, negative years where representable, and range boundaries,
- exact arithmetic before and after the Unix epoch,
- monotonic ordering and cross-clock rejection,
- representative historical timezone changes and future transition rules,
- unique, ambiguous, and nonexistent local times,
- zones with non-hour and historical second-level offsets,
- stable aliases and unknown names,
- deterministic behavior under a pinned timezone database,
- strict RFC 3339 and ISO 8601 round trips and malformed inputs,
- and equivalent serial and parallel demand of provider queries.

## Navigation

- [I/O and platform standard-library surface](io-and-platform-surface.md)
- [I/O and platform service architecture](io-and-platform-services.md)
- [I/O and platform language semantics](../language/io-and-platform-services.md)
