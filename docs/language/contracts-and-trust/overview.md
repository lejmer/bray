# Overview

Bray separates five related concepts:

- trusted implementation capability,
- ordinary contract requirement,
- static constraint,
- trusted caller obligation,
- predicate.

A trusted implementation capability is authority used inside a trusted declaration body.

An ordinary contract requirement is a checkable condition expressed in predicate-expression context.

A static constraint is a compile-time condition expressed in static predicate context.

A trusted caller obligation is a condition the ordinary checker cannot prove and that must be supplied, preserved, or
acknowledged.

A predicate is a named contract-level relation used in requirements, guarantees, constraints, and trusted obligations.

Trusted implementation power and trusted caller obligations are independent.

A declaration can use trusted implementation capabilities internally while exposing an ordinary safe API.

A declaration exposes trusted caller obligations only when its contract says so.

Trusted declarations are permitted only in trusted modules.

Trusted module rules are defined in [Trusted modules](../modules-and-packages/trusted-modules.md).

## Navigation

- [Language index](../index.md)
- [Contracts and trust index](../contracts-and-trust.md)
- Next: [Contract clauses](contract-clauses.md)
