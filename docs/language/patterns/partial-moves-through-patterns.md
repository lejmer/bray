# Partial moves through patterns

A consuming pattern can move fields or payloads out of a subject.

General partial-move rules are defined in [Partial moves](../ownership-and-borrowing/partial-moves.md).

Moving a field or payload is an ownership operation.

`matches`, `if let`, and `while let` observe their subjects. They do not partially move a subject merely by matching
it. A successful conditional binding names the observed part under the ordinary borrow and copy rules. Moving a
non-copyable part requires a consuming context such as `match consume`. A pattern may inspect only the initialized
parts required by its structural tests and bindings. Destruction still follows the subject's actual initialized state.

Moving parts out requires ownership of the subject and no conflicting active borrows.

After a partial move, the subject is partially initialized.

A partially moved value can be reinitialized or consumed by a rule that accounts for its state.

Destruction of a partially moved value destroys only the still-initialized parts.

For unions, destruction follows the active variant and the initialized state of its payload fields.

## Navigation

- [Language index](../index.md)
- [Patterns index](../patterns.md)
- Previous: [Pattern operation modes](pattern-operation-modes.md)
- Next: [Condition refinement](pattern-refinement.md)
