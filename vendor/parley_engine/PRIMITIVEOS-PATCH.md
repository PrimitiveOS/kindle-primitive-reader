# PrimitiveOS Reader patch

This directory contains `parley_engine` 0.12.0 from
https://github.com/linebender/parley/tree/6e1143dda817360dd3a2a64f9133c6e5dfabd881/parley_engine
(crates.io archive SHA-256
`b74c0d373e462e504ad4436710a39f099e18e848e1ee711f70cd2dcd5a69b290`).
The upstream Apache-2.0 and MIT licenses are included unchanged.

Reader adds an optional, bounded sequence of virtual Unicode bidi controls to
analysis. The resolver filters the controls' levels before returning levels
for the original text. Invalid, unsorted, unbalanced or oversized control
sequences fall back to the upstream source-only analysis. The patch does not
insert control bytes into the original text or its byte-offset coordinates.
