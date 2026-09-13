# Compiler diagnostics

Diagnostics are immutable compiler product data shared by command-line tools, editors, and automation. Their structure
preserves the cause and source relationships of a failure independently of its presentation. Language validity belongs
to the language specification. Rendering and invocation policy do not change it.

## Structured records

Compiler phases emit stable message identities and typed arguments. `bray-messages` owns user-facing text. Source
references, semantic identities, target properties, and failure categories retain their meaning until rendering rather
than becoming fragments of English inside compiler logic.

A diagnostic carries its severity, primary origin, labels, related locations, notes, and suggested corrections.
Generated data preserves an origin chain so a report can explain the source or metadata decision that caused it. Absence
of a source location is explicit. Product and infrastructure failures need not invent one.

Stable diagnostic kinds identify conditions across wording changes. Individual record identities identify emissions.
Rendered codes are a tooling presentation of those kinds. These identities serve different purposes.

Suggestions carry edits and applicability separately from their explanatory messages. Tooling can validate and apply a
correction without interpreting localized prose. Producers own the evidence for a correction. Renderers preserve it.

## Diagnostic results

`bray-diagnostics` owns the shared value-plus-diagnostics result and persistent ordered diagnostic collections.
`bray-compilation` owns the query machinery around them. The neutral result contains no caching, phase scheduling, or
rendering policy. A phase-specific wrapper is justified only by an additional semantic invariant or relationship.

Invalid source may yield an error-aware semantic value with diagnostics. Cancellation and infrastructure failure are
outer query outcomes. They do not masquerade as recovered source values.

Each stage owns its local collection. Dependent results compose immutable references to prerequisite collections rather
than copying or republishing their records. Parallel tasks build independently, and the publication boundary assembles
required results in stable source-and-stage order. A failed or cancelled query publishes neither a partial value nor a
partial diagnostic collection.

The same collection graph supports focused queries and compilation-wide reporting. Structural deduplication accounts for
the complete diagnostic context, including related locations and corrections, rather than rendered wording or the
identity of an individual emission. Parallel completion order never determines the published order.

## Ownership and recovery

The phase with enough information to identify a violation owns its diagnostic. Later phases consume explicit recovery
data and suppress consequences that would merely repeat an earlier failure. Lowering and output phases report problems
in their own representations, targets, artifacts, or tools. Source-language decisions stay in semantic analysis.

Recovery preserves a useful distinction between valid data, missing source data, placeholders, and compiler invariant
violations. It retains enough context for further analysis without treating invalid source as ordinary valid input.
Invariant failures are not a source recovery mechanism.

## Localization and presentation

The message layer owns wording, argument order, plurals, lists, quotations, and locale-specific grammar. The intended
catalog distribution is a validated bundle identified by schema and message-set revision. Each installed locale supplies
the registered messages with their typed argument signatures. Adding a locale changes catalog data, not compiler phases.

Rendering receives explicit ordered locale preferences using BCP 47 identities. Negotiation considers installed exact
and less-specific language identities, with an installed English catalog as the deterministic fallback. Host locale is
not an implicit compiler input. A missing fallback message or mismatched signature is a distribution invariant failure.

Command and editor adapters own filtering, warning promotion, output format, display limits, color, and locale
selection. Text, JSON, and editor output are projections of the same structured records, not separate diagnostic
implementations.

## Related documents

- [Compiler architecture](compiler-architecture.md)
- [Diagnostic contributor guidance](../contributing/diagnostics.md)
