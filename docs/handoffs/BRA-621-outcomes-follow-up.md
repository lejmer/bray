# Follow-up findings from outcomes verification

The requested trusted-obligation repair is limited to the outcomes fixtures and their native audit catalog. These findings were left unchanged, as requested.

## Existing structural style warnings

On 2026-10-10, `cargo xtask style` completed successfully but reported existing `module-too-large` warnings in compiler, runtime, and xtask modules, plus four `unused-exemption` warnings in `crates/bray-checker/src/dependency/check.rs` and `crates/bray-checker/src/selection/check.rs`.

The warnings are recorded in `target/bra-621-outcomes/style.log`. Reproduce with `cargo xtask style`. The style pass made no additional file changes. Module restructuring and exemption removal are outside the trusted-obligation repair.
