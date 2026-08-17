## Maintain the references of this skill

Keep each language reference concise, evidence-backed, and independently useful.

- Link to specification pages with absolute `https://github.com/lejmer/bray/blob/develop/docs/language/...` URLs so installed copies work outside this repository.
- Do not reproduce the specification. Summarize only what an agent must retrieve quickly to use the feature correctly.
- Give each feature reference a compact, exhaustive example set that covers every language form in its routed domain at least once. Prefer one cohesive scenario, but use independent fragments when source contexts or product modes are mutually exclusive.
- Treat lexical and syntax grammar indexes as compact routing summaries rather than feature references. They do not need exhaustive examples. Record only the non-obvious rules an agent may need before opening the linked prose specification or plain EBNF grammar.
- Examples may assume stated imports and support declarations, and bodies may use `// ...` for irrelevant behavior. Every shown form must remain valid, and non-`unit` bodies should retain the required result exit.
- Compress bodies and explanation before omitting a language form. The example is the agent's syntax inventory, not merely a representative tutorial.
- Describe Bray through the forms it actually provides. Prefer a positive rule that names the correct construct or selection mechanism, and avoid naming hypothetical syntax solely to reject it.
- Make every example follow the repository's [Bray source style guide](https://github.com/lejmer/bray/blob/develop/docs/contributing/bray-style-guide.md).
- Maximize semantic coverage per line, not brevity per line. Keep meaningful names, semantic paragraphs, and enough structure for a human to scan correctly.
- Use periods, commas, or explicit conjunctions in prose. Do not use semicolons or em dashes outside code examples.
- Open with one dense, task-relevant orientation sentence and close with a short `Remember` line covering the decisive semantics and common traps. Use `Why` only when a feature has a non-obvious selection rationale.
- Verify every addition against authoritative repository sources. Update stale entries to describe the current language, not compatibility guidance or implementation history.
