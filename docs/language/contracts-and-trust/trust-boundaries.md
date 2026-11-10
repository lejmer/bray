# Trust boundaries

Calling a declaration with trusted caller obligations requires the obligations to be established at that program point
or visibly acknowledged at a trust boundary.

Expression-level trust boundary syntax is defined in
[Trust boundary expressions](../expressions/trust-boundary-expressions.md).

```bray
trusted expression
```

A trust boundary visibly accepts the exact trusted predicate requirements of its operand. Its scope is exactly the
operand expression, including the whole block when the operand is a block.

A caller must acknowledge a trusted obligation unless language-defined contract reasoning already establishes it at
that program point. A safe wrapper around trusted implementation code requires no acknowledgement. A declaration
exposing trusted caller obligations requires acknowledgement or proof.

Guarantees introduced solely by acknowledgement do not become conditions after the operand completes. Conditions
independently established by the operand's ordinary result, pattern, or `ensures(...)` behavior flow out under ordinary
guarantee-context rules.

A trust boundary expression must acknowledge at least one trusted caller obligation required by its operand.

If the operand has no trusted caller obligation, the boundary is rejected as redundant.

`trusted expression` does not grant trusted implementation capabilities and does not satisfy ordinary requirements.

## Navigation

- [Language index](../index.md)
- [Contracts and trust index](../contracts-and-trust.md)
- Previous: [Trusted witness values](trusted-witness-values.md)
- Next: [Obligation propagation](obligation-propagation.md)
