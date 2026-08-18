# Lifecycle ordering

When all lifecycle kinds apply to the same value, the lifecycle order is:

```text
construct -> ordinary use -> enter -> with body -> exit -> finalize -> destruct -> represented-part destruction
```

Not every value passes through every lifecycle step.

`enter` and `exit` occur only for scoped use through a `with` expression.

`finalize` occurs only for values with finalization obligations.

`destruct` occurs when ownership ends for a fully initialized value with destruction behavior.

Represented-part destruction is field destruction for product types and active-payload destruction for union types.

Default represented-part destruction order is defined by the represented type's rules.

Lifecycle ordering is preserved on ordinary scope exit, early control-flow exit, panic propagation, cancellation, and
failed partial construction.

Async scope exit adds a task cancellation-broadcast phase before reverse lifecycle resolution. Panic and cancellation
cleanup can apply the abnormal finalization fallback after an attempted fallible finalizer fails. Destruction ordering
remains unchanged.

If evaluation exits before construction completes, already-initialized parts, temporaries, acquired capabilities, and
partially initialized storage are resolved by the corresponding ownership, destruction, finalization, and capability
rules.

Partial-value lifecycle behavior is defined by [Partial values and replacement](partial-values-and-replacement.md).

Static values use the same value-level lifecycle order. Static instances add owner-level ordering from the static
lifecycle dependency graph. If `A` can require `B` during cleanup, `A` completes finalization, destruction, and
represented-part destruction before cleanup of `B` begins.

Thread-local static cleanup for one attachment completes on its exact native thread before product-static cleanup can
resolve any product storage required by that attachment. Independent eligible static instances within one cleanup domain
use the formally defined static cleanup order key. Dependencies across attachment, consumer-product, and
provider-product domains impose domain precedence, while independent domains can clean concurrently and have no global
execution order.

## Navigation

- [Language index](../index.md)
- [Lifecycle index](../lifecycle.md)
- Previous: [Lifecycle selection](lifecycle-selection.md)
- Next: [Construction](construction.md)
