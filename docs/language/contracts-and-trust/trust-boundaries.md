# Trust boundaries

Calling a declaration with trusted caller obligations requires the obligations to be established at that program point or visibly acknowledged at a trust boundary.

Expression-level trust boundary syntax is defined in [Trust boundary expressions](../expressions/trust-boundary-expressions.md).

```bray
trusted expression
```

A trust boundary expression is a trust boundary for its operand expression.

It visibly accepts the trusted caller obligations required by that operand.

The compiler records the exact trusted predicate requirements accepted at the boundary.

The caller must visibly accept the trusted obligation unless language-defined contract reasoning already establishes it at that
program point.

A call to a safe wrapper around trusted implementation code does not require caller acknowledgement.

A call to a declaration with trusted caller obligations does require caller acknowledgement or proof.

The boundary scope is exactly the operand expression.

For a block operand, the scope is the block.

Trusted guarantees introduced solely by the boundary do not become conditions after the operand completes.

Conditions independently established by the operand's ordinary result, pattern, or `ensures(...)` behavior flow out according to ordinary guarantee-context rules.

A trust boundary expression must acknowledge at least one trusted caller obligation required by its operand.

If the operand has no trusted caller obligation, the boundary is rejected as redundant.

`trusted expression` does not grant trusted implementation capabilities and does not satisfy ordinary requirements.

## Navigation

- [Language index](../index.md)
- [Contracts and trust index](../contracts-and-trust.md)
- Previous: [Trusted witness values](trusted-witness-values.md)
- Next: [Obligation propagation](obligation-propagation.md)
