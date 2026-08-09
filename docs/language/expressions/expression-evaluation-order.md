# Expression evaluation order

Runtime expression evaluation is source-order by default.

Binding, path resolution, overload selection, type checking, contract checking, and compile-time argument checking are checking steps.

Checking steps do not create runtime evaluation steps.

Runtime subexpressions are evaluated in the order they are written unless a more specific expression rule defines a narrower order.

Unary and binary operands are evaluated left to right.

For `&&` and `||`, the left operand is evaluated first, and the right operand is evaluated only when required by short-circuit boolean semantics.

Tuple elements are evaluated left to right.

Array elements are evaluated left to right.

Conditional expressions evaluate the condition before evaluating the selected branch body. Unselected branch bodies are not
evaluated.

While expressions evaluate the condition before each attempted iteration. The while body is evaluated only when the condition is
true. The while else body is evaluated only when the condition evaluates to `false`.

For expressions evaluate the source expression once before iteration begins. The for body is evaluated once for each produced
element until the source is exhausted or control leaves the for expression. The for else body is evaluated only when iteration
reaches natural exhaustion.

Repeated-element array expressions evaluate the repeated element expression before initializing repeated elements according to the repeat contract.

Function call callee expressions are evaluated before call arguments.

Method receiver expressions are evaluated before method arguments.

Static function callee path resolution is checked before runtime evaluation and has no runtime evaluation step.

Async invocation evaluates the receiver or callee, explicit arguments, and omitted defaults before the resulting `Future<T>` owns
that state. `Future<T>.start()` evaluates and consumes its receiver before the new task can observe the transferred frame.

Lambda expressions do not capture enclosing local bindings.

Lambda bodies are evaluated only when the produced callable value is called.

Boolean fold expressions evaluate their operand once and then iterate it through the selected `Iterable` and `Iterator` contracts.

`all(...)` stops iterating after the first `false` element.

`any(...)` stops iterating after the first `true` element.

Explicit call arguments are evaluated in source order.

Named argument binding is separate from argument evaluation order.

Named arguments bind by parameter name, but evaluate in the order written by the caller.

Positional arguments bind by position, and also evaluate in source order.

Omitted parameter defaults are evaluated after explicit arguments, in parameter declaration order.

Supplied struct field initializer expressions are evaluated in source order.

Omitted struct field defaults are evaluated after supplied field initializers, in field declaration order.

Supplied union payload initializer expressions are evaluated in source order.

Omitted union payload defaults are evaluated after supplied payload initializers, in payload field declaration order.

Runtime construction arguments for type-form construction expressions are evaluated in source order.

Omitted runtime construction defaults are evaluated after explicit runtime construction arguments, in construction parameter declaration order.

With expressions evaluate their initializer once, apply `enter`, evaluate the body while the scoped bindings are live, and apply
`exit` before body-local destruction, expression completion, or propagation of the body's control-flow outcome.

Compile-time arguments, storage policy types, trait applications, overload declarations, type arguments, and path resolution have no runtime evaluation order.

The compiler may reorder implementation work only when the reordering preserves observable Bray semantics.

Observable Bray semantics include effects, ownership, borrowing, mutation authority, destruction, finalization, capability checking,
trusted obligations, contract-checking results, and control-flow outcomes.

Specific expression forms also define these evaluation conditions:

- a match expression evaluates its subject once,
- a for source expression is evaluated once before iteration,
- an `each` source expression is evaluated once before iteration,
- a struct or variant field default is evaluated when that field is omitted,
- a box construction expression evaluates the contained value before initializing indirect storage,
- a with expression evaluates its initializer once and applies `exit` on every body exit,
- a guard is evaluated after structural pattern matching and before selecting the arm body,
- an assertion expression evaluates its condition first and evaluates its message only when the condition is false.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Flow-sensitive contract reasoning](flow-sensitive-contract-reasoning.md)
- Next: [Summary](summary.md)
