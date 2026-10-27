# Ryu

The ordinary Bray implementation in `standard-library/std/src/format/real_decimal.bray` and its power cache derive from
[Ryu](https://github.com/ulfjack/ryu), Copyright 2018 Ulf Adams, under the included Boost Software License 1.0.

The referenced upstream Git blobs are `2bcd9e5d0b740839499b4cc1a4bd7440d5b7ee82` (`ryu/generic_128.c`) and
`6f66ba0637872ac4f12644663b111deba2b9efd0` (`ryu/generic_128.h`). The Bray port uses explicit non-overflowing limb
arithmetic and the exact interval rules for implicit-leading-bit IEEE formats.

This port accepts implicit-leading-bit IEEE representations only. The smallest normal keeps the same lower-neighbor
spacing as subnormals. Trailing-zero divisibility uses the power of five selected by interval scaling, including power
zero. These interval details are checked against exact rational midpoint intervals for every finite binary16 magnitude
and edge and sampled values of the wider formats.
