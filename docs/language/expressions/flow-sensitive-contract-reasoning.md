# Flow-sensitive contract reasoning

The language rules determine which conditions are available before and after each expression.

Examples:

- successful pattern matching refines an active union variant,
- match arm selection refines the subject state,
- branch conditions can make ordinary boolean conditions available,
- assertions can make asserted conditions available,
- construction can make type and variant guarantees available,
- assignment can make previous guarantees about the destination unavailable,
- mutation can make guarantees about affected storage unavailable,
- movement can make guarantees about moved values unavailable,
- destruction can make guarantees about destroyed storage unavailable,
- trusted declarations can establish trusted guarantees through `ensures(...)`.

An implementation must produce the same acceptance and rejection decisions required by these rules. The specification does not
prescribe an internal representation for the available conditions.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Expression destruction and finalization behavior](expression-destruction-and-finalization-behavior.md)
- Next: [Expression evaluation order](expression-evaluation-order.md)
