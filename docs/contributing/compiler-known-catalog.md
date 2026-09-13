# Editing the compiler-known catalog

Edit `.braydef` inputs under `crates/bray-compiler-known/catalog/`, and add new files to its source manifest. Use nearby
entries for the private wrapper syntax. Embedded declaration and type fragments use the ordinary Bray grammar.

Keep keys explicit and stable. Give independently identified members an `owner`, and recognized standard-library entries
an explicit owner-relative `identity`. Representation, implementation, operation, and availability fields name closed
typed roles. Add behavior in its owning compiler phase, not in the catalog.

Run `cargo xtask compiler-known generate`, then `cargo xtask compiler-known check`. Do not edit generated Rust by hand.
The check validates freshness, identities, ownership, target views, and complete semantic interpretation.

See [catalog design](../design/compiler-known-catalog.md) for the representation and publication model and [xtask
commands](xtask.md#compiler-known-catalog) for automation.
