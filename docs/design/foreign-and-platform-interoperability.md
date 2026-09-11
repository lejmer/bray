# Foreign And Platform Interoperability

This document defines the implementation architecture for Bray interoperability with C ABIs, dynamically loaded code,
foreign callbacks, native resources, and target-specific operating-system facilities. The language-level ABI,
raw-memory, trust, and target gate rules remain defined by the language specification.

The contract extends the standard-library and platform boundaries defined by
[Standard library packages and toolchain artifacts](standard-library.md) and
[I/O and platform services](io-and-platform-services.md). It does not create a second foreign type system, resource
model, linker, runtime, or source-level trust mechanism.

## Goals

The design must provide:

- ordinary standard-library declarations for common C values, strings, errors, resources, dynamic libraries, and
  callbacks,
- exact ownership and borrowing for every native resource and callback context,
- target-aware dynamic loading without ambient library discovery,
- generated callback entry that contains panics and initializes foreign caller threads,
- explicitly target-gated low-level operating-system modules,
- typed operational failures without host-authored user-facing prose,
- deterministic compiler results, artifacts, and diagnostics,
- and a narrow allocation of responsibility across source, compiler, runtime, platform, and native-shim layers.

The design must preserve the existing language contracts for `@abi(...)`, `@layout(...)`, `@link(...)`, `@symbol(...)`,
`extern`, raw pointers, trusted capabilities, target properties, and module contribution gates.

## Principles

Foreign interoperability follows these rules:

- Foreign interaction is explicit in declaration types, callable contracts, effects, and target availability.
- A native address or integer is not ownership merely because a platform API calls it a handle.
- Every acquired resource has one typed owner until ownership is explicitly transferred or released.
- Borrowed symbols, handles, pointers, and callback contexts cannot outlive their owner.
- Expected foreign and operating-system failures are typed values, not panics or compiler diagnostics.
- A foreign panic boundary never unwinds through a non-Bray ABI frame.
- Portable standard-library modules do not expose operating-system constants or raw platform representations.
- Target-specific modules use ordinary `@target(...)` contributions and leave no fallback declaration on unsupported
  targets.
- The compiler validates and lowers declared contracts. It does not invent missing foreign ownership or failure
  contracts.
- Native code provides irreducible ABI normalization and operating-system mechanisms, not portable policy.

## Public Package Layout

The public surface belongs to the `std` package:

| Module           | Responsibility                                                                                               |
|------------------|--------------------------------------------------------------------------------------------------------------|
| `std.ffi`        | Shared foreign failures, resource-boundary helpers, callback ownership, and explicit raw-boundary operations |
| `std.ffi.c`      | C scalar wrappers, NUL-terminated strings, C-compatible status helpers, and C conversion utilities           |
| `std.dynamic`    | Target-aware dynamic-library ownership and lifetime-bound typed symbol lookup                                |
| `std.os.windows` | Windows-only low-level handles, constants, and direct system facilities                                      |
| `std.os.linux`   | Linux-only low-level descriptors, constants, and direct system facilities                                    |
| `std.os.darwin`  | Darwin-only low-level descriptors, constants, and direct system facilities                                   |

These are ordinary Bray modules. None becomes ambient or compiler-known merely because it wraps a foreign mechanism. The
compiler recognizes only the language-owned ABI, layout, pointer, capability, target-property, and runtime-role
identities already assigned by their specifications.

Portable facilities remain in their owning modules. Files stay in `std.fs`, child processes stay in `std.process`,
sockets stay in `std.net`, clocks stay in `std.time`, and threads stay in `std.thread`. Those modules may use `std.ffi`
internally but must not re-export raw operating-system surfaces as portable contracts.

`std.os` has no universal fallback surface. A target-specific child module is available only when its module
contribution gate matches `target.identity.SYSTEM`. Shared source can place multiple gated contributions under one
module identity, but declarations whose native representation or semantics differ remain in the target module that owns
them.

## C Interoperability

The language already owns callable ABI selection, aggregate layout, raw pointer forms, extern declarations, symbol
identity, link requirements, and foreign-call capability checking. `std.ffi.c` builds conveniences over those contracts
rather than replacing them.

### C Values

C scalar declarations are transparent standard-library value types selected by the exact `target.c` properties defined
in the target-profile language contract. The complete family covers:

- signed and unsigned `char`, `short`, `int`, `long`, and `long long`,
- `size_t`, `ptrdiff_t`, and `wchar_t`,
- the target C boolean representation,
- and the C floating representations supported by the target profile.

Each wrapper has one target-selected scalar field and an explicit transparent layout. Construction from a Bray scalar is
an explicit checked or wrapping operation according to the named conversion. Observation returns the exact underlying
Bray scalar. No general module-level type alias is introduced, and a C wrapper is not implicitly interchangeable with an
equal-width Bray scalar.

Each property maps one C type to a stable Bray scalar spelling or `"unavailable"`. The target-profile validator
guarantees equal value representation, size, alignment, and C callable classification. The standard library then selects
one target-gated transparent wrapper by an exact property comparison. A wrapper is unavailable when its property is
`"unavailable"`. Compiled package interfaces record every `target.c` property that affects a public representation.

The wrapper registry is exhaustive for `CHAR`, `SIGNED_CHAR`, `UNSIGNED_CHAR`, `SHORT`, `UNSIGNED_SHORT`, `INT`,
`UNSIGNED_INT`, `LONG`, `UNSIGNED_LONG`, `LONG_LONG`, `UNSIGNED_LONG_LONG`, `SIZE`, `PTRDIFF`, `WCHAR`, `BOOL`, `FLOAT`,
`DOUBLE`, and `LONG_DOUBLE`. Each wrapper is generated or selected from its own exact property rather than inferred from
width, a neighboring C type, or the compiler host. The registry validates availability, Bray scalar identity,
transparent layout, and by-value C ABI classification together. An `"unavailable"` mapping produces no declaration and
no approximate byte-product substitute.

Fixed-width C APIs should use Bray's fixed-width scalar types directly when the C declaration guarantees that exact
representation. The named C wrappers are for declarations whose ABI follows target C types rather than fixed widths.

The public wrapper names are `Char`, `SignedChar`, `UnsignedChar`, `Short`, `UnsignedShort`, `Int`, `UnsignedInt`,
`Long`, `UnsignedLong`, `LongLong`, `UnsignedLongLong`, `Size`, `PointerDifference`, `WideChar`, `Boolean`, `Float`,
`Double`, and `LongDouble`. Each available wrapper exposes `value()` with its exact selected Bray scalar result. An
`*_exact` const constructor accepts only that exact scalar representation. Integer wrappers additionally expose
`*_checked` from the corresponding `i128` or `u128` value domain and `*_wrapping` with explicit wrapping semantics.
Checked range failure returns `ConversionError.OutOfRange`. Boolean and real wrappers have no wrapping conversion.

### C Strings

`std.ffi.c` distinguishes borrowed and owned NUL-terminated storage:

- `BorrowedNarrowString` is a borrowed, immutable sequence ending in exactly one accessible NUL unit.
- `OwnedNarrowString` owns a NUL-terminated sequence and resolves its allocation on destruction.
- Wide C strings use separately named unit-width types and do not inherit UTF-8 behavior.

Construction from bytes or text validates interior NUL units, terminal NUL storage, capacity arithmetic, and the
selected encoding policy. Conversion to Bray `string` is fallible. A narrow C string is interpreted as UTF-8 only when
the caller requests text conversion, and a native path is not silently converted through a C string.

```bray
let bytes = try trusted std.ffi.c.OwnedNarrowString.from_bytes(source);
let text = try trusted std.ffi.c.OwnedNarrowString.from_utf8(&value);
let borrowed = try std.ffi.c.borrow_string(terminated_units);
let decoded = try borrowed.to_string();
```

`OwnedNarrowString.from_bytes` copies raw narrow units, while `OwnedNarrowString.from_utf8` names text encoding.
`BorrowedNarrowString.to_string` decodes those units as UTF-8. The `borrow_string` overload accepts a validated narrow
byte slice and, when the target has a wide C character type, a validated wide-unit slice. `OwnedWideString.from_string`
and `BorrowedWideString.to_string` select UTF-16 or UTF-32 from the target's `WCHAR` representation without exposing
platform-specific operation names. Invalid surrogate sequences, invalid Unicode scalar values, and interior NUL
characters return `StringError` rather than being replaced or reinterpreted.

Borrowed narrow strings expose raw storage through `as_bytes` and `as_bytes_with_nul`. Borrowed wide strings use
`as_units` and `as_units_with_nul`. Owned narrow and wide strings produce their dependency-preserving views through
`as_borrowed`.

Borrowing a raw pointer from a borrowed narrow or wide string retains the source dependency, including when the view
came from an owned C string. Constructing a borrowed C string from a raw pointer is trusted and requires an explicit
readable extent or a caller obligation that permits bounded terminator search. The safe surface never performs an
unbounded scan of untrusted storage.

### Error Boundaries

`std.ffi.ForeignError` carries a stable category and an optional numeric native code. Stable categories include
unsupported, invalid input, unavailable symbol, unavailable library, permission denial, exhaustion, interrupted
operation, incompatible ABI, invalid resource, and other target failure.

Host-authored prose is not a semantic field. A target module may provide an explicit formatting helper for a native
code, but that text is observational and locale-dependent. Compiler diagnostics use structured message identities and
typed arguments instead.

C error conventions remain declaration-specific. Sentinel returns, thread-local error variables, out parameters, and
status records are adapted by ordinary trusted Bray wrappers into typed results. The compiler does not infer an error
convention from a symbol name or result type.

## Foreign Resources And Native Handles

A foreign resource is a value whose lifecycle is controlled partly by code outside Bray. A native handle is one possible
raw representation of such a resource. It is not itself the ownership contract.

The public library does not define one universal `NativeHandle` that erases resource kind, target, close operation,
affinity, or invalid representation. Concrete owners such as `DynamicLibrary`, a Windows kernel-handle owner, or a Linux
file-descriptor owner remain distinct types.

Every owning resource type defines:

- the raw representation accepted from its provider,
- the exact valid and invalid raw states,
- whether ownership is unique, retained, duplicated, or externally shared,
- the operation that releases or transfers ownership,
- the borrowing and thread-affinity rules,
- the synchronization required for shared observation or mutation,
- and the typed failure behavior of explicit release.

Successful acquisition returns an initialized owner. Failure returns no owner. A raw invalid value is validated at the
trusted boundary and never represented as an apparently live owner. Public absence uses `Option`. Operational failure
uses `Result`.

Explicit release consumes the owner and returns its fallible result. Destruction provides the contractually required
best-effort cleanup for an owner that was not explicitly released, but cannot turn a fallible release into observable
success. APIs for which release failure must be observed require explicit resolution before normal completion.

Borrowing a resource exposes only the operations admitted by that borrow. Raw-handle observation is a trusted operation
whose result carries the owner's dependency. Ownership transfer consumes the owner and returns a target-specific
transfer value or passes the value directly to the receiving boundary. It does not copy the raw representation while
leaving two apparent owners.

Resource duplication is a distinct fallible operation supplied only where the target contract can create an independent
ownership obligation. Retaining an externally reference-counted resource is likewise explicit and returns a new owner
only after the foreign retain operation succeeds.

## Dynamic Libraries And Symbols

`std.dynamic.DynamicLibrary` is a non-copyable owner of one loaded module. Opening a library accepts an explicit path or
an explicit target-defined system-library identity plus a typed load policy. It never searches the current directory,
process environment, registry, parent directories, package graph, or network unless the selected policy explicitly names
a target facility with that behavior.

Load policy states visibility, binding, namespace, and executable-image requirements only where the selected target
supports them. Unsupported combinations fail before invoking the loader. Platform defaults are represented by an
explicit default policy whose meaning is fixed for that target artifact.

`DynamicLibrary.open` overloads the path and system-library forms. The explicit first argument selects the arm, while
the load policy remains named and shared by both forms.

`library.symbol<T>(name)` accepts an exact native symbol name and an ABI-qualified requested type. A successful lookup
returns a `DynamicSymbol<T>` borrowed from the library owner. The symbol cannot outlive the library, and the library
cannot be closed while a symbol borrow remains active. `library.close()` borrows the owner mutably and releases the
module. Success establishes `DynamicLibrary.complete`. An error retains any module ownership that the platform
still holds, so the caller can retry or transfer the owner. `library.is_complete()` exposes completion to ordinary
code through checked Boolean postconditions. Consuming `library.into_handle()` transfers its native handle and
establishes completion for the old Bray owner.

The requested type is part of the trusted lookup boundary. The loader can establish only that an address exists. It
cannot prove a foreign function's signature, data layout, ownership, effects, or failure behavior. Safe wrappers
therefore keep typed lookup inside a trusted module and publish an ordinary Bray callable or owner whose complete
contract is declared in source.

A callable symbol records its exact callable ABI. A data symbol records its pointee type, mutability, initialization,
and lifetime obligations. Converting an untyped address to either form is trusted and rejects representations the
selected target cannot express.

Lookup failure retains the library owner. A library with active symbol borrows
cannot be closed or transferred. A loaded Bray product also cannot enter cleanup while an external entry, callback,
callable, owner, or other transitive root can reach its code or storage. A static-owned edge inside the active teardown
set instead orders consumer cleanup before provider cleanup. After entry closure and external-root quiescence, close
drives exact-thread and product-static cleanup, releases internal provider edges, and then releases the loaded image.
Destruction resolves an otherwise live library according to the foreign-resource rules.

Dynamic loading is target-conditional through the boolean `target.platform.dynamic_loading` property. The public value
and error types remain available for generic signatures, while open and lookup operations are unavailable when that
property is false. The property is part of the language-defined closed `target.platform` surface and participates in
target gates and compiled-interface dependencies like the existing filesystem and child-process properties.

## Foreign Callbacks

A foreign callback is a non-Bray ABI entry through which external code invokes Bray code. Its callable type, ownership,
context, thread-entry behavior, reentrancy, panic policy, and lifetime must all be explicit.

An explicit exported foreign entry contract selects a callback boundary. ABI visibility alone leaves target platform
fallback definitions as direct Bray call targets.

### Plain Function Pointers

A plain ABI-qualified callback value can refer only to a noncapturing static callable whose ABI and complete parameter
and result representations match the requested callback type. The compiler emits a stable trampoline when the Bray
callable body requires a Bray calling convention internally.

A plain function pointer carries no context ownership. It is valid for the linked image lifetime and cannot represent a
capturing lambda, borrowed local state, or a dynamically unloadable callable without another owner preserving that
dependency.

### Explicit Callback Contexts

Bray callable values remain capture-free. A stateful foreign callback uses an explicit `std.ffi.CallbackContext<State>`
owner plus a static ABI-qualified entry whose first parameter is the context pointer. Context construction consumes one
explicit `State` value. It does not inspect a lambda or capture an enclosing binding. Borrowing the foreign pair
preserves the context-owner dependency and produces the supplied static entry pointer plus the opaque pointer expected
by that entry.

A trusted entry reconstructs a borrow of `State` through the recognized `std.ffi.callback_state<State>(context)`
operation. The compiler accepts that operation only inside an exported ABI callable whose matching context parameter is
live, records the borrow against the context owner, and lowers it without inventing hidden callable state. The ordinary
exported-callable wrapper supplies foreign-thread entry and panic containment. There is no separate capture-synthesis
hook.

The context owner is non-copyable. Its explicit state is checked by ordinary ownership, run-transfer, synchronization,
and thread-affinity rules. A wrapper cannot hide a borrowed local in a retained context. Transfer to foreign ownership
consumes the Bray owner and is permitted only when the foreign API exposes a release callback or deregistration
operation with an exact once-only ownership contract.

Registering a callback does not imply that the foreign API retains it. The wrapper for that API declares whether
invocation is call-only, scoped, retained until explicit deregistration, or ownership-transferring. The callback
representation follows that declared lifetime rather than guessing from the C signature.

For retained callbacks, successful deregistration must guarantee that the foreign provider will begin no new invocation.
The wrapper then waits for every already-entered invocation to leave before destroying the context. If an API cannot
provide that quiescence contract, a Bray-owned retained context cannot safely wrap it. Invocation after release violates
the foreign API precondition before Bray entry. A trampoline never dereferences retired storage merely to diagnose that
violation.

### Callback Entry

A generated callback trampoline performs these steps in order:

1. Enter one invocation against the still-live context under the registration's synchronization contract.
2. Acquire the provider-product entry dependency and reuse or establish the foreign caller's exact native-thread
   attachment.
3. Establish a synchronous callback root through the selected private runtime ABI.
4. Reconstruct the explicit state borrow and ABI parameters without duplicating ownership.
5. Invoke the static Bray adapter under its declared execution, effect, trust, and reentrancy requirements.
6. Convert the normal result to the exact foreign ABI representation.
7. Resolve callback-root lifecycle state, leave the in-flight invocation, and complete the matching outer detach before
   returning to foreign code.

Nested entry reuses one attachment. The outer detach waits for exact-thread dependencies and pinned work, cleans
thread-local statics on the same native thread, and only then releases the attachment. Product unload closes new entry
and attachment before waiting for quiescence.

Thread initialization is a private platform/runtime mechanism and does not depend on the public `std.thread.Thread<T>`
abstraction. It establishes only the execution requirements declared by the callback boundary. A callback requiring
main-thread, blocking, compute, or other execution requirements is rejected or routed only when the foreign API contract
supplies those requirements.

Concurrent invocation is allowed only when the callback's state and synchronization contracts admit it. Reentrant
invocation is separate from concurrent invocation and must be declared by the wrapper for the foreign API.

An uncaught Bray panic is contained before the foreign frame. The callback adapter declares the ABI-representable
failure value or foreign termination policy used when normal return is still required. A trampoline never fabricates an
arbitrary zero value, never unwinds through foreign code, and never resumes a failed Bray continuation.

## Target-Specific Operating-System Modules

Target-specific modules expose facilities that cannot honestly be portable: native constants, raw flags, handle
operations, system-call-shaped records, and OS-specific control operations.

Each source contribution uses an exact gate over `target.identity.SYSTEM`, for example a Windows contribution compares
the selected system with `"windows"`. The gate is evaluated before declaration identity and body checking. Package
interfaces retain the target properties that affect every public target-specific declaration.

Native constants have one authority: generated target-specific Bray source checked into the standard-library tree.
`cargo xtask` generation reads a short manifest whose shared POSIX and operating-system sources are combined with one
source for each exact target. These files describe the pinned target SDKs. The generator writes authoritative source
with the complete input digest and fails verification when regeneration differs. Compilation never reads the compiler
host's headers. The generated
source bytes participate in the ordinary package source and artifact digests, so cross compilation and repeated builds
use the same target constants even when the host operating system differs. Target metadata may validate a value but
never supplies an alternate constant definition.

Each target's generated module provides exact constants, C scalar mappings, layouts, callbacks, functions, imported
storage, and dynamic-symbol helpers for that target.

Target modules may expose typed raw values needed to call operating-system APIs, but ownership remains in explicit owner
types. Constants do not create resources, integer conversion does not transfer ownership, and matching numerical values
across operating systems do not make their types interchangeable.

Portable modules can implement a portable operation through target modules internally. Their public result, ownership,
ordering, and failure behavior remains the portable module's contract. A portable API must not leak a target-specific
constant, raw handle, path representation, or error enum merely because one implementation uses it.

## Compiler And Runtime Responsibilities

### Compiler

The compiler owns:

- validation of ABI-qualified declarations and callable values,
- target ABI and layout mapping,
- `@link(...)` and `@symbol(...)` semantic contracts,
- foreign import and export classification,
- target-gate evaluation and target-property dependency recording,
- checked callback-context transfer, affinity, effect, and lifetime contracts,
- exported-wrapper generation,
- immutable native-link requirements,
- and structured diagnostics for invalid source or incompatible target contracts.

The compiler does not open dynamic libraries while checking source, inspect symbols to infer callable types, choose
resource release policy, translate native errors into public errors, or make unavailable target declarations appear
portable.

### Standard Library

Ordinary trusted Bray source owns:

- public foreign values, owners, errors, policies, and conversions,
- C string validation and storage,
- resource lifecycle state machines,
- dynamic-load and symbol-lookup policy,
- explicit callback-context ownership and registration lifetimes,
- target-specific low-level wrappers,
- and adaptation from private mechanism results to public typed results.

Trusted source uses the narrowest required raw-memory, foreign-call, and platform capabilities. Public callers do not
inherit those capabilities merely because an implementation uses them internally.

### Runtime

The execution runtime owns callback-root entry, panic containment, run context, cancellation context, lifecycle
resolution, and safe return to the foreign boundary. It does not own C string policy, dynamic-library search policy,
target constants, or public resource types.

The runtime reuses the existing synchronous-root and runtime-thread contracts. Foreign callbacks do not introduce a
second task scheduler, detached execution model, or source-visible runtime object.

### Platform And Native Support

`bray-platform` owns safe typed native mechanisms that can be implemented without violating the repository's Rust safety
policy for compiler-host tooling and trusted runtime layers. Trusted standard-library code uses generated direct target
bindings. `bray-platform-abi` owns link integration for the isolated third-party temporal provider.

Operations that require irreducible unsafe system calls, loader casts, header macros, unusual calling conventions, or
assembly trampolines belong in narrow native C, C++, or assembly shims beneath `bray-platform-abi`. Such a shim
validates its ABI inputs, contains native exceptions or unwinding, and returns fixed-layout status records. It does not
own portable search, lifecycle, callback, or error policy.

## Platform-Service Roles

The private platform-service catalog identifies operations that the runtime may override or that the temporal provider
supplies. The dynamic-loader roles and their exact schemas are defined in `docs/design/io-and-platform-services.md`.
They use role IDs `0x0801` through `0x0804`, the `dynamic_library` handle class, the `dynamic_loading` capability, and
the existing structured status record. The trusted Bray implementation retains target loader error codes in
`native_code`. It never returns host prose.

Foreign caller-thread attachment is not a platform-service role. Callback entry reuses
`bray-platform::RuntimeThreadScope` and the existing synchronous-root runtime ABI. Target-specific duplication or
transfer is added to the closed catalog only together with a public target-module consumer and an allocated role schema.
This contract allocates no speculative generic resource role.

Every role has one typed ABI schema, ownership behavior, blocking behavior, target availability rule, and stable role
identity. Role bindings are selected from the standard-library product manifest and validated like existing stream,
filesystem, process, clock, and entropy roles.

Callback trampolines are compiler-generated code and callback-root entry is an execution-runtime role. They are not
dynamic-loader platform roles. C scalar and string conveniences are ordinary Bray code and require no platform role
unless they call a target service.

## Artifacts And Linking

Static foreign imports contribute immutable native-link requirements during product formation. Dynamic lookup
contributes no link-time symbol requirement for the looked-up symbol, but the selected target artifact must provide the
loader service and its native dependencies.

Package interfaces encode public target dependencies, ABI-qualified callable types, layouts, ownership contracts, and
required capabilities. Package implementation artifacts retain executable templates and native-boundary metadata needed
by imported generic or exported foreign callables.

Generated callback and export symbols use deterministic names derived from stable semantic identities. Repeated builds
with equal inputs produce equal symbols, link requirements, platform-role requirements, and native artifacts.

Loading an arbitrary library at runtime does not add it to the compiler package graph, grant access to its declarations,
or weaken the checked type of a looked-up symbol.

## Diagnostics And Failures

Invalid ABI types, layouts, callback captures, target gates, link requirements, and unavailable declarations produce
structured compiler diagnostics. Diagnostic arguments carry symbols, types, ABIs, target identities, resource kinds, and
source spans rather than pre-rendered English.

Runtime loader, symbol, resource, and native-call failures are ordinary standard-library error values. A numeric native
code is preserved when useful. Failure cleanup preserves still-live owners and reports cleanup incidents through the
existing structured runtime contract when normal typed return is no longer possible.

Malformed package interfaces, platform-role manifests, or native artifacts are rejected at their existing validation
boundaries. The compiler does not fall back to host discovery, a different calling convention, or a similarly named
symbol.

## Conformance

Conformance coverage must include:

- C scalar width, signedness, layout, call, result, and conversion behavior on every supported target family,
- borrowed and owned narrow and wide C strings, including malformed and unterminated inputs,
- exact native error categories and retained numeric codes,
- resource acquisition, borrowing, transfer, duplication, explicit release, destruction, and invalid-handle rejection,
- dynamic open, missing library, typed symbol lookup, missing symbol, active-symbol close prevention, and unload
  cleanup,
- noncapturing and capturing callbacks, foreign-thread entry, concurrent and reentrant invocation, release races, and
  panic containment,
- target-gated module presence and absence under native and cross-target profiles,
- deterministic callback symbols, role requirements, diagnostics, and repeated artifacts,
- and representative native fixtures that prove cleanup after each failure boundary.

Native conformance fixtures use fixed small C ABI surfaces and do not make host-specific behavior part of portable
library semantics. Tests distinguish compiler diagnostics, typed operational failures, panic containment, and
infrastructure failures.

## Dependency And Conformance Order

Delivery follows this dependency order. Every completed step must use the final contracts and ownership boundaries
defined above:

1. Publish C ABI value, string, error, and conversion conveniences over the existing language ABI machinery.
2. Establish stable foreign-resource and native-handle ownership helpers.
3. Implement dynamic libraries, foreign callbacks, and target-specific operating-system modules over those foundations.
4. Add end-to-end native conformance for every mechanism and cleanup path.

Dynamic libraries additionally depend on the handle ownership contract even though a loader can expose an address
without it. Foreign callbacks depend on private runtime-thread entry, not on the public native-thread standard-library
API.

## Non-Goals

This contract does not provide:

- automatic C header parsing or binding generation,
- host-header discovery during cross compilation,
- implicit library search or package installation,
- reflection-based symbol typing,
- a universal untyped native-handle owner,
- transparent conversion between Bray and foreign strings,
- automatic ownership inference from C signatures,
- unwinding through non-Bray ABI frames,
- portable aliases for target-specific constants or errors,
- COM, Objective-C, JNI, Python, or another higher-level foreign object model,
- or a second runtime, scheduler, linker, package graph, or diagnostic renderer.

Higher-level interoperability layers can consume these contracts later. They must define their own object identity,
retention, threading, exception, and error rules rather than overloading the C and operating-system foundations.
