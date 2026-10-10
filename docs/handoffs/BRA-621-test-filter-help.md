# Test filter help does not match selection behavior

Found while verifying the Windows transfer API tests on 2026-10-10. The user requested repairing only those fixtures and recording separate issues in handoffs. No filter implementation or CLI help was changed.

`bray test --help` describes each positional filter as matching a test's qualified identity. However, `TestFilter::NameContains` in `crates/bray-test-protocol/src/filter.rs` matches only `declaration.name()`. A module-name filter therefore selects no entries even when the catalog contains tests in that module.

The retained Windows API product contains all three `bray.standard_library_tests.platform_transfer_windows` entries. Running it with filter `platform_transfer_windows` reports 159 discovered tests and zero selected. Selecting the three function names in separate batch plans runs them successfully. Multiple filters in one plan are combined with `all`, so distinct function names in one invocation also select zero entries.

Decide the intended filter contract, then align the CLI help and implementation. Preserve the existing selection rules unless a deliberate behavior change is required.

Evidence is retained under `target/bra-621-windows-transfer-tests/`:

- `focused-native-final.log` records successful compilation and zero selections with the module-name filter.
- `focused-native-execution.log` records zero selections with three distinct function-name filters in one invocation.
- `transfer-batch.log` records one passing test in each of the three separate plans.
- `reused-identities.txt` records the verified runtime and standard-library toolchain identities.

The retained executable and catalog are in `target/bra-621-windows-transfer-tests/workspace/build/x86-64-windows/release/std/`. Native test execution must run outside the Codex sandbox. Ignored local artifacts do not survive a machine reset.
