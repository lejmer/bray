# Fuzzing

This directory owns Bray fuzz harnesses, corpora, minimized reproducers, and fuzz-tool configuration.

Fuzzing is separate from ordinary unit and integration tests. Normal compiler tests belong in crate test modules, `tests/`, or
focused fixtures. Fuzzing can produce large corpora, generated artifacts, and tool-specific state, so that material stays here.

Fuzz targets should focus on compiler robustness:

- lexing,
- parsing,
- syntax recovery,
- lossless source reconstruction,
- diagnostic production,
- binder entry points.

Invalid fuzz input should produce diagnostics or recovery data, not compiler panics.

When a fuzz failure reveals a stable behavior that should stay covered, add a small deterministic regression test to the ordinary
test suite as well.
