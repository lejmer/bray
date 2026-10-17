# Diagnostic message rules

The messages in this crate are part of Bray's user interface. Write them for Bray programmers, not compiler implementers.

## Required context

- Identify the affected source-level construct. Name it when it has a name, describe its Bray syntax category otherwise, and attach its source location whenever one is available.
- State expected and actual values when that distinction explains the failure.
- Preserve the most specific structured context supplied by an earlier compiler phase. Do not collapse identifiers, roles, categories, counts, types, targets, or other useful fields into a payload-free diagnostic category.
- For legitimate input or operational failures involving generated material, identify the user-visible compilation unit or product and preserve the specific cause. Do not expose the generated representation as though the user could inspect or repair it.
- When the user can correct the problem, put the concrete next action in structured supporting material such as a help note or suggestion. Do not append remediation to the primary error message.
- Violated compiler invariants are not diagnostics. Assert or panic at the owning boundary with useful debugging context. Do not convert impossible states into recoverable errors or user-facing messages.

## Language

- Use Bray source terminology and concepts documented for Bray users.
- Do not expose compiler implementation terms such as HIR, MIR, bound or checked nodes, lowering, semantic selection, storage identities, control-flow edges, terminators, frame descriptors, or internal numeric IDs.
- Do not use vague predicates such as `invalid`, `inconsistent`, `unavailable`, `missing`, or `unsupported` without saying exactly what is affected and what requirement was violated.
- Keep titles concise, but put necessary identification and explanation in the diagnostic detail, labels, notes, or related locations rather than omitting it.
- Keep concerns separated: the primary error identifies and explains the failure; labels and related locations identify relevant source; notes provide context; help notes and suggestions provide remediation.

## Review and tests

- Review the fully rendered message, including its title, detail, labels, notes, suggestions, and source locations. Reviewing only message IDs or diagnostic enum variants is insufficient.
- Add rendering tests for new diagnostic branches. Tests must verify that the result identifies the affected source-level construct and does not leak compiler-internal terminology.
- Treat any diagnostic that leaves a Bray programmer asking “which thing?” or “what does that mean?” as incomplete.
