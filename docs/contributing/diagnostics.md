# Compiler diagnostics

Bray diagnostics are a locale-neutral protocol between compiler producers and user-facing tools. A producer reports a
`DiagnosticKind`, typed arguments, source relationships, and mechanically valid corrections. `bray-messages` owns the
text seen by users. Compiler crates must not assemble English messages or encode English phrases as generic string
arguments.

## Choose exact structure

Use the narrowest diagnostic kind that describes the failure the producer observed. Split catch-all kinds when the
producer can distinguish causes with different corrections. Keep domain values typed, including paths, types, syntax
kinds, target requirements, counts, revisions, and I/O categories.

A source diagnostic should identify its primary span and attach a primary label that explains the role of that span.
Labels annotate the current failure. A separate source location that explains an origin, prior declaration, conflict,
or requirement must use `DiagnosticRelatedLocation` so text, JSON, and LSP clients preserve the relationship.

The primary message states only what failed and why. Every recovery step, next action, and hint belongs in a typed
`DiagnosticNote`. A concrete source correction may additionally use `DiagnosticSuggestion`, but a suggestion does not
replace the explanatory note and action prose never belongs in the primary message.

Use `DiagnosticNote` when information beyond the primary message materially improves understanding, explains a
constraint, or tells the user what to do next. Do not add generic notes that merely restate the error. Reuse a note kind
only when its guidance is accurate for every diagnostic that emits it.

## Suggestions and edits

Use `DiagnosticSuggestion::try_edits` for corrections with source edits. Applicable suggestions require a nonempty
ordered set of non-overlapping edits. Edits with the same start position conflict. Use `Manual` for useful guidance
without edits.

Applicability describes confidence that applying the edit preserves intent:

- `MachineApplicable` means the correction is mechanically determined
- `MaybeApplicable` means the edit is plausible but user intent is not proven
- `Manual` provides guidance that tooling must not apply

Text and JSON output preserve every suggestion. The language server offers structurally valid applicable suggestions as
quick fixes and respects the diagnostic context and requested code-action kinds supplied by the client.

## Producer tests

Tests must exercise the real production or conversion boundary and call `assert_goal_state_diagnostic_kind` on the
produced bag with the exact expected `DiagnosticKind`. Every distinct kind emitted by a producer path needs its own
exact assertion. The assertion checks the kind's exhaustive quality contract, verifies that the primary message contains
no recovery instruction, and renders every message component. Missing arguments in the primary message, labels, notes,
related locations, or suggestions fail the test. Generic bag-wide assertions may supplement this check, but do not
establish readiness coverage.

Add rendered text and JSON snapshots for representative user experiences. Add an LSP protocol test when a change affects
related information or safe code actions. Catalog-only diagnostics created directly with `Diagnostic::new` do not
establish producer coverage.

Run the diagnostic readiness audit after adding or changing a diagnostic kind:

```console
cargo xtask readiness diagnostics
```

The audit requires every registered kind to have quality-asserted executable producer coverage and requires the text,
JSON, and LSP surfaces to retain the structured information.
