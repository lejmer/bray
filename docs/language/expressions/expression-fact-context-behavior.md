# Expression fact-context behavior

Expressions can add, remove, or refine facts in the fact context.

Examples:

- successful pattern matching refines an active union variant,
- match arm selection refines the subject state,
- branch conditions can establish ordinary boolean facts,
- assertions can establish ordinary contract facts,
- construction can establish type and variant facts,
- assignment can invalidate facts about the destination,
- mutation can invalidate facts about affected storage,
- movement can invalidate facts about moved values,
- destruction can invalidate facts about destroyed storage,
- trusted declarations can establish trusted facts through `ensures(...)`.

Fact-context behavior is flow-sensitive.

Facts are tied to values, storage identities, lifetimes, capabilities, and versions.

Facts expire when the values or storage they depend on change in a way that can affect truth.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Expression destruction and finalization behavior](expression-destruction-and-finalization-behavior.md)
- Next: [Expression evaluation order](expression-evaluation-order.md)
