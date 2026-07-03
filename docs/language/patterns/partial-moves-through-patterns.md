# Partial moves through patterns

A consuming pattern can move fields or payloads out of a subject.

General partial-move rules are defined in [Partial moves](../ownership-and-borrowing/partial-moves.md).

Moving a field or payload is an ownership operation.

Moving parts out requires ownership of the subject and no conflicting active borrows.

After a partial move, the subject is partially initialized.

A partially moved value can be reinitialized or consumed by a rule that accounts for its state.

Destruction of a partially moved value destroys only the still-initialized parts.

For unions, destruction follows the active variant and the initialized state of its payload fields.

## Navigation

- [Language index](../index.md)
- [Patterns index](../patterns.md)
- Previous: [Pattern operation modes](pattern-operation-modes.md)
- Next: [Fact-context refinement](fact-context-refinement.md)
