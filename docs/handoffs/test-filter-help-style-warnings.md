# Repository style warnings observed during the test filter help fix

Found on 2026-10-10 while running the required `cargo xtask style` command for
`BRA-621-test-filter-help.md`. The command succeeded. These warnings concern
unchanged code and were left for separate work at the user's explicit request.

The structural checker reported unused `context-erasing-failure-conversion`
exemptions at:

- `crates/bray-checker/src/dependency/check.rs:276` and `:283`
- `crates/bray-checker/src/selection/check.rs:170` and `:217`

It also reported production modules above its 800-line warning threshold:

| Module | Production lines |
| --- | ---: |
| `crates/bray-checker/src/analysis/build.rs` | 832 |
| `crates/bray-checker/src/analysis/guarantee/flow.rs` | 1457 |
| `crates/bray-checker/src/analysis/storage_flow/check/analysis/memory.rs` | 806 |
| `crates/bray-checker/src/constant/template_evaluation/evaluator.rs` | 818 |
| `crates/bray-checker/src/execution_guarantees/condition.rs` | 903 |
| `crates/bray-checker/src/execution_guarantees/normalize.rs` | 853 |
| `crates/bray-checker/src/storage/expression/planning.rs` | 863 |
| `crates/bray-codegen-llvm/src/translation/unit/core.rs` | 805 |
| `crates/bray-codegen-llvm/src/translation/unit/effect/host.rs` | 821 |
| `crates/bray-lowering/src/lowering/cleanup/outcomes.rs` | 801 |
| `crates/bray-lowering/src/lowering/expression/control/iteration.rs` | 828 |
| `crates/bray-native-artifact/src/resolver.rs` | 835 |
| `crates/bray-package-interface/src/semantic/validation/template.rs` | 821 |
| `crates/bray-runtime/src/native/run.rs` | 802 |
| `crates/bray-symbols/src/value/substitution_apply/apply.rs` | 828 |
| `xtask/src/performance/corpus.rs` | 820 |
| `xtask/src/standard_library/command/core.rs` | 811 |

Re-run `cargo xtask style` to verify the current warnings before taking on this
work. Evaluate module cohesion before splitting files. Remove obsolete exemptions
only after confirming that the surrounding conversions preserve their failure
context. These warnings do not require changing test filter help or selection.
