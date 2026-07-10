# Compiler diagnostics

This document defines the design contract for Bray compiler diagnostics.

Diagnostics are compiler product data. They are not Bray source-language semantics.

The language specification defines which programs are valid and what valid programs mean. Diagnostic design defines how the
compiler represents, orders, localizes, tests, and reports invalid or suspicious input.

---

## Goals

Diagnostics should be:

- structured enough for tools and tests,
- precise enough to point at the source construct responsible for the problem,
- stable enough for automation,
- locale-neutral until rendering,
- deterministic under parallel execution,
- recoverable enough to continue checking when continuation improves output,
- cheap enough to emit from every compiler phase without ad hoc formatting work.

Compiler logic must not construct user-facing English text.

Compiler logic emits structured diagnostic records with message IDs and typed arguments.

User-facing text is rendered by the locale-aware message layer.

---

## Diagnostic Records

A diagnostic record has:

- a stable diagnostic identity,
- severity,
- primary source span when one exists,
- optional labels,
- optional notes,
- optional related locations,
- optional suggestions,
- typed message arguments,
- phase ownership,
- deterministic ordering key.

The diagnostic record is immutable after publication.

Diagnostic records can be created through phase-local builders, but published records must not expose mutable builder state.

Diagnostic records must not contain pre-rendered user-facing sentences.

Diagnostic records can contain compiler-owned typed references such as source IDs, spans, syntax kinds, symbol IDs, type IDs,
declaration IDs, operator kinds, counts, literal kinds, and target fact names.

When a diagnostic needs a source spelling, it should carry a source reference or stable source slice, not an already quoted English
phrase.

---

## Diagnostic Results

`bray-diagnostics` owns the neutral immutable value-plus-diagnostics wrapper used by lazy semantic facts.

Conceptually:

```rust
pub struct DiagnosticResult<T> {
    value: T,
    diagnostics: DiagnosticBag,
}

impl<T> DiagnosticResult<T> {
    pub const fn value(&self) -> &T;
    pub const fn diagnostics(&self) -> &DiagnosticBag;
    pub fn into_parts(self) -> (T, DiagnosticBag);
}
```

The wrapper contains no phase logic, caching, or rendering policy. `bray-compilation` owns lazy evaluation, dependency tracking,
cancellation, caching, and publication around it.

Invalid user source can produce a valid `DiagnosticResult<T>` containing an error-aware semantic value and error diagnostics.
Cancellation and compiler infrastructure failure are outer query outcomes and must not be represented as an absent value or an
error-aware source result.

Symbol facts, checked semantic units, and other lazy semantic facts use this shared wrapper rather than defining equivalent
phase-named copies. A category-specific result type remains appropriate when it adds a stronger root, identity, or relationship
contract rather than merely pairing one value with one diagnostic bag.

---

## Identity

A diagnostic identity is the stable kind of a compiler report.

Diagnostic identities are not source locations and are not rendered messages.

Two diagnostics with the same identity can have different spans, arguments, labels, notes, and suggestions.

The identity should be stable across wording changes.

Diagnostic identities should be organized by owning compiler phase or feature area.

Examples:

- lexical invalid character,
- parser missing expression,
- declaration duplicate name,
- binding unresolved name,
- type mismatch,
- borrow conflict,
- trusted obligation not discharged,
- target-unavailable declaration.

Rendered diagnostic codes can be derived from diagnostic identity, but the identity remains the compiler-internal stable key.

The code format is a user-interface and tooling contract owned by the diagnostics crate.

---

## Severity

Diagnostics use these severities:

- `error`,
- `warning`,
- `note`,
- `help`.

An `error` prevents successful compilation of the affected product.

A `warning` does not prevent successful compilation unless the invocation policy promotes warnings to errors.

A `note` provides supporting information for another diagnostic.

A `help` provides an advisory action or explanation for another diagnostic.

Standalone notes and help messages are allowed only for compiler-driver reporting that is not tied to a source-language validity
failure.

Phase logic should not choose severity based on localized wording.

Severity is part of the structured diagnostic record.

---

## Spans And Labels

A primary span identifies the source range most directly responsible for a diagnostic.

Some diagnostics have no primary span, such as errors about missing package metadata, unavailable target profiles, or linker input
selection.

A label attaches structured explanation to a span.

Labels can be primary or secondary.

Labels must use message IDs and typed arguments.

Labels should point to source ranges that help explain the diagnostic, not merely repeat the primary span.

Related locations connect a diagnostic to other source ranges or external inputs involved in the same problem.

Examples of related locations:

- the previous declaration in a duplicate declaration error,
- the trait member declaration for an invalid implementation member,
- the borrow creation point for a later conflicting mutation,
- the target gate that makes a declaration unavailable,
- the package manifest entry that selected a target profile.

Generated or synthesized compiler data should preserve its origin chain so diagnostics can point back to the source or metadata
that caused it.

---

## Suggestions

A suggestion is a structured source change or user action associated with a diagnostic.

A suggestion has:

- a message ID,
- typed message arguments,
- one or more source edits when the suggestion is source-applicable,
- applicability,
- target span or insertion point when source-applicable.

Applicability values are:

- `machine_applicable`,
- `maybe_applicable`,
- `manual`.

`machine_applicable` means the edit is valid for the exact source input and can be applied without changing program intent beyond
the diagnostic fix.

`maybe_applicable` means the edit is syntactically plausible but can require user judgment.

`manual` means the compiler can describe the action but cannot provide a complete edit.

Suggestions that edit source must preserve trivia when possible.

Suggestions must not rely on localized text parsing.

---

## Localization

Diagnostics are locale-neutral until rendering.

The diagnostics crate owns the structured records and rendering contracts.

The `bray-messages` layer owns localized text.

The initial supported locale is English.

Adding a locale should require adding message catalogs and locale rules, not changing parser, binder, checker, lowering, codegen,
or emitter logic.

Localization owns:

- wording,
- argument ordering,
- plural forms,
- list formatting,
- quotation style,
- grammar-specific phrasing,
- locale-specific punctuation.

Typed message arguments must preserve meaning.

Compiler logic should pass `TypeId`, `SymbolId`, `SyntaxKind`, `OperatorKind`, `usize`, `Span`, `TargetFactId`, or similar typed
values rather than pre-rendered phrases.

If a requested locale is unavailable, fallback behavior must be explicit and deterministic.

---

## Phase Ownership

Each compiler phase owns diagnostics for violations it has enough information to report accurately.

The lexer owns lexical diagnostics.

The parser owns syntax diagnostics and parser recovery diagnostics.

Declaration discovery owns declaration-shape diagnostics that do not require body checking.

Symbol construction owns semantic identity conflicts and symbol table construction diagnostics.

Binding owns name resolution, path resolution, lexical-scope, shadowing, and reference-target diagnostics.

Semantic checker services own type, trait, overload, conversion, ownership, borrowing, initialization, lifecycle, contract,
capability, const-evaluation, and target-availability diagnostics.

Lowering, IR validation, code generation, and emission diagnostics must describe compiler, target, backend, artifact, or external
tool issues. They must not introduce new source-language semantic decisions.

A later phase should not duplicate an earlier phase's diagnostic.

When a later phase depends on invalid earlier data, it should consume explicit recovery data and suppress follow-on noise.

---

## Recovery

Recovery exists to improve diagnostic output while preserving compiler invariants.

Recovered compiler data must be explicit.

Do not represent recovered or erroneous state as ordinary valid data unless the node, symbol, bound node, or fact carries an
explicit error marker.

Downstream phases must be able to distinguish:

- valid data,
- missing data caused by user input,
- placeholder data introduced for recovery,
- compiler invariant violations.

Recovery should preserve enough source context to avoid vague later diagnostics.

Recovery should avoid cascades where one source error produces many unrelated reports.

Compiler invariant violations are not recovery cases.

Compiler invariant violations should fail loudly during development and tests.

---

## Ordering

Diagnostic ordering must be deterministic.

Parallel worker completion order must not affect published diagnostic order.

The final ordering key should include:

- package or product order,
- source input order,
- declaration order when available,
- primary span order when available,
- phase order when needed to break ties,
- diagnostic identity when needed to break ties,
- creation sequence only within a deterministic single task.

Diagnostics without source spans should use a deterministic product, package, metadata, or invocation-order key.

Different runs over the same inputs, target profile, dependency graph, and compiler options should produce diagnostics in the same
order.

---

## Deduplication

Diagnostic deduplication uses structured diagnostic facts, not localized rendered text.

Two diagnostics are duplicates only when their severity, stable kind, primary span, labels, notes, and typed arguments are all
equal. The diagnostic record ID is not part of the duplicate key because it identifies one emitted record, not the source fact
being reported.

Diagnostics that render to similar or identical prose must remain distinct when they refer to different source facts, spans,
labels, notes, related locations, suggestions, or typed arguments.

When related locations, suggestions, or other structured fields are added to diagnostic records, they must become part of the
duplicate key.

---

## Suppression And Policy

The core diagnostic record does not decide command-line policy.

Invocation policy can:

- promote warnings to errors,
- filter warnings,
- select output format,
- select locale,
- select color and terminal formatting,
- limit displayed diagnostics,
- emit machine-readable output.

Policy must not change source-language validity rules.

Policy changes should be represented outside parser, binder, and semantic checker logic.

---

## Testing

Diagnostic tests should cover:

- diagnostic identity,
- severity,
- primary span,
- labels,
- notes,
- related locations,
- suggestions,
- typed message arguments,
- deterministic ordering,
- rendered output where rendering behavior matters.

Tests should validate structured records before rendered text when the behavior being tested is compiler logic.

Rendered-output snapshots are useful for message catalogs, CLI output, LSP output, and user-facing formatting.

Rendered-output tests should not be the only tests for semantic diagnostic behavior.

Invalid-input tests should verify that recovery avoids unrelated cascaded diagnostics.

Parallel diagnostic tests should verify deterministic ordering under different worker budgets.

Fuzzing should include diagnostic production and recovery paths.

Fuzz-generated invalid input should produce diagnostics or explicit recovery data, not compiler panics.

---

## Relationship To Compiler Architecture

The compiler architecture document defines phase boundaries, immutable data ownership, scheduling, and reusable compiler
primitives.

This document owns the detailed diagnostic data and reporting design.
