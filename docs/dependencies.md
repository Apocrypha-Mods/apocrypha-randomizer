# Dependencies

Every dependency, its licence and why it is here.

| Crate / source | Licence | Used for |
| --- | --- | --- |
| apoc-fsparams (Apocrypha desktop) | proprietary, Nova's own | regulation.bin: AES, DCX, BND4, PARAM read/write/merge |
| fstools-rs (soulsmods) | MIT + Apache-2.0 | reference / possible dependency: archives, name hash, MSB |
| zstd, flate2, aes, cbc, rsa, num-bigint, rand_chacha, clap | MIT / Apache-2.0 | compression, crypto, seeded RNG, CLI |
| oozextract | MIT label, **disputed** (port of GPL ooz) | Kraken decode, feature-flagged only, legal check before shipping |

**Do not use:** SoulsFormats / SoulsFormatsNEXT, ooz, zao/ooz (GPL); Paramdex
data (no licence); fromsoftware-rs ("MIT NON-AI", not OSI); SoulsRandomizers
(all rights reserved).
