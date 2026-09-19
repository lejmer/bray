# Editing the compiler-known catalog

Edit `.braydef` inputs under `crates/bray-compiler-known/catalog/`, and add new files to its source manifest. Use nearby
entries for the private wrapper syntax. Embedded declaration and type fragments use the ordinary Bray grammar.

Keep keys explicit and stable. Give independently identified members an `owner`, and recognized standard-library entries
an explicit owner-relative `identity`. Representation, implementation, operation, and availability fields name closed
typed roles. Add behavior in its owning compiler phase, not in the catalog.

## Regenerate and validate

Do not edit generated Rust by hand. After changing the catalog, run:

```text
cargo xtask compiler-known generate
cargo xtask compiler-known check
```

Generation updates the Rust source and digest. The check validates freshness, identities, ownership, target views, and
complete semantic interpretation. To check generated files without semantic validation or writing files, run:

```text
cargo xtask compiler-known generate --check
```

Generation rejects an executable whose embedded definitions differ from the selected checkout and writes no files.
Rebuild xtask from that checkout before retrying.

See [catalog design](../design/compiler-known-catalog.md) for the representation and publication model and [repository
tasks](xtask.md) for other workflows.
