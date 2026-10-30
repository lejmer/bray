# Numeric runtime implementation and cost analysis

This analysis records the numeric runtime implementation and measured costs from BRA-659, as part of the Bray native
runtime migration. Build commands and packaging checks are documented in the
[runtime artifact guide](../contributing/runtime-artifacts.md).

The `RealPower`, `ComplexMultiply`, `ComplexDivide` and `ComplexPower` runtime roles use the Bray bootstrap
implementation. The role catalog remains the authority for ownership, symbols and native signatures. The compiler's
constant evaluator retains `bray-numeric`; produced programs select the Bray implementation through ordinary native
artifact demand.

The native boundary passes a real component as two `u64` words, low word first, and a complex value as two such records.
Their C layouts are 16 and 32 bytes with alignment 8. The entry's opaque authority covers initialized operands, a
supported IEEE width and an exclusive output destination. Typed projections grant access only to these records.

## Arithmetic and storage

All arithmetic operates on integer words. It preserves IEEE binary16, binary32, binary64 and binary128 without a host
floating-point conversion. The shared encoder implements nearest-even rounding, gradual underflow, signed zero,
overflow and canonical positive quiet NaNs.

Finite complex multiplication forms exact four-word products. Signed accumulation retains complete products whenever
cancellation is possible; a sticky low bit preserves the rounding direction of a much smaller term. Division uses this
fixed storage only when both numerators and the divisor remain exact. Wider exponent gaps use owned integer storage,
and the exact division remainder reaches final IEEE encoding without an intermediate floating-point rounding.

Owned integers keep sixteen words inline and use initialized `List<u64>` storage for larger precisions. Small products
use schoolbook multiplication; sufficiently large balanced products use difference-form Karatsuba with one recursive
workspace. Normalized division reuses a word reciprocal and bounded quotient corrections. Integer square root refines
a half-precision root with one full-width Newton step and at most one correction, so its work grows geometrically.

Power operations preserve the language's exceptional-value and principal-branch rules. Integer powers, square roots
and exact axes avoid unnecessary transcendental constants. Guarded arithmetic reuses initialized mantissas in series
loops. Ordinary precisions reduce logarithm and arctangent arguments to nearby sixteenths using immutable binary
constants. Larger logarithms and constants use arithmetic-geometric means, and large exponentials use Newton
refinement with increasing precision. Sine and cosine share argument reduction and reconstruction. Reduction retains
an exact product and refines pi when cancellation would otherwise lose a small remainder.

The sixteen-word constant floors retain at least sixty additional bits beyond their largest lookup precision. The optional
[constant certifier](../../tools/numeric-constants.cpp) uses directed MPFR intervals at 2,048-bit precision. It requires
MPFR/GMP development headers and libraries and is independent of the compiler and produced programs. Each output row
names the constant and prints its integer floor in hexadecimal; the Bray tables store that integer in low-word-first
order. The pi table uses scale 1,022; the logarithm and arctangent tables use scale 1,024.

## Boundary coverage

The runtime smoke test calls the four actual native exports from C and compares them with the compiler's numerical
oracle. It covers every binary16 encoding, exceptional values, cancellation, subnormal midpoint decisions, general
powers and large-exponent phase reduction. Its workload is a fixed set of arithmetic cases, rather than a wall-clock
budget. Six-target checks verify the native record layouts, signatures and Bray function bodies.

An ordinary executable separately exercises the native profiles' enabled language widths through operators, including agreement
with constant expressions. Source and native standard-library reuse pass through the normal emitter's synchronous
runtime audit. That audit retains native-unit inclusion reasons and authenticates the actual linker-map members against
the Bray implementation artifacts. Numeric demand does not introduce execution-service demand.

## Linux x86-64 cost checkpoint

A release consumer calling all four runtime roles changes as follows. Both runs use LLVM 22.1.8 and the same source;
compiler preparation and runtime packaging are excluded from consumer build time. The delivered compiler builds differ,
so build durations are observations rather than an isolated comparison of compiler performance.

| Cost | Rust provider | Bray provider |
| --- | ---: | ---: |
| Executable bytes | 5,236,544 | 1,062,944 |
| Code/read-only bytes | 1,063,746 | 971,419 |
| Data + BSS bytes | 16,288 | 3,160 |
| Complete consumer build seconds | 1.054 | 0.909 |
| Median process milliseconds, 20 interleaved launches | 1.473 | 1.005 |
| Allocation count through shutdown | 5,963 | 7 |
| Cumulative requested allocation bytes | 177,208 | 2,016 |
| Peak usable allocation bytes | 8,080 | 1,960 |
| Live usable allocation bytes at exit | 96 | 0 |

Allocation measurements use the Linux/glibc process interposer described in the
[synchronous footprint checkpoint](../contributing/runtime-artifacts.md#synchronous-footprint-checkpoint). The native plan's inclusion
reasons and the actual map's 521 retained runtime members authenticate against the Bray implementation artifacts;
no opaque or project-owned Rust runtime unit is selected. Numeric demand has no execution capabilities or lanes.

The following complete binary64 kernel measurements use CPU time, changing input bit patterns and matching output
checksums. Values are medians of seven samples, with 10,000 multiply/divide operations and 100 powers per sample.
Bray includes its owned-value cleanup and performs no allocation per operation. The Rust reference calls the compiler's
numerical API. The C++ reference uses MPFR at 256-bit guarded precision with reusable scratch values and its existing
constant caches. These are arithmetic implementations with comparable guarded composition, not host `double` math.

| Operation | Bray nanoseconds | Rust nanoseconds | MPFR nanoseconds |
| --- | ---: | ---: | ---: |
| Complex multiply | 323 | 1,147 | 306 |
| Complex divide | 573 | 2,197 | 541 |
| Real power | 19,670 | 237,840 | 8,440 |
| Principal complex power | 70,820 | 542,940 | 22,450 |

Common multiply/divide are close to MPFR on this workload. Powers improve substantially over the replaced Rust path
but remain about 2.3–3.2 times slower than this MPFR reference. This checkpoint does not establish parity for every
input, precision or machine. A separate binary128 phase-reduction case with an imaginary exponent of `2^16000`
matches the compiler oracle and takes about 100 milliseconds in Bray versus 383 milliseconds in Rust on this host.
That case exercises the larger-precision algorithms rather than only the fixed storage paths.

See [runtime artifact verification](../contributing/runtime-artifacts.md#verify-packaging-and-native-linking) for the containing workflow.
