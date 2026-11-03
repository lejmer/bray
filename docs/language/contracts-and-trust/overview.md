# Overview

Bray distinguishes the authority to implement an operation from the obligations of its callers. Its contract concepts
have the following roles:

| Concept | Role |
| --- | --- |
| Trusted implementation capability | Authority used inside a trusted declaration body. |
| Ordinary contract requirement | A checkable condition expressed in predicate-expression context. |
| Static constraint | A compile-time condition expressed in static predicate context. |
| Trusted caller obligation | A condition the ordinary checker cannot prove, which must be supplied, preserved, or acknowledged. |
| Predicate | A named contract-level relation used in requirements, guarantees, constraints, and trusted obligations. |

Trusted implementation power and trusted caller obligations are independent. A declaration can use trusted
capabilities internally while exposing an ordinary safe API. Callers have trusted obligations only when the
contract exposes them.

Trusted declarations are permitted only in [trusted modules](../modules-and-packages/trusted-modules.md).
The [contract clauses](contract-clauses.md) specify how requirements, constraints, guarantees, and implementation
capabilities are declared.

## Navigation

- [Language index](../index.md)
- [Contracts and trust index](../contracts-and-trust.md)
- Next: [Contract clauses](contract-clauses.md)
