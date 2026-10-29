# Dependencies

Every dependency, its licence and why it is here.

## In use

| Crate / source | Licence | Used by | For |
| --- | --- | --- | --- |
| apoc-fsparams (Apocrypha desktop) | proprietary, Nova's own | fs-bytes, fs-dcx, fs-param | `rd.rs`, `Error`, `dcx.rs`, `param.rs` and the regulation crypto lifted and widened; merge still to come |
| aes 0.8 | MIT OR Apache-2.0 | fs-dvdbnd | AES-128-ECB of archive entries |
| base64 0.22 | MIT OR Apache-2.0 | fs-dvdbnd | PEM bodies of the archive keys |
| num-bigint 0.4 | MIT OR Apache-2.0 | fs-dvdbnd | raw RSA (modpow) of the `.bhd` headers |
| rayon 1 | MIT OR Apache-2.0 | fs-dvdbnd, fs-tool | parallel header decryption; whole-game checks |
| thiserror 2 | MIT OR Apache-2.0 | fs-bytes | the shared error type |
| zstd 0.13 (zstd: BSD-3-Clause) | MIT; BSD-3-Clause | fs-dcx | zstd read and write |
| flate2 1 (miniz_oxide) | MIT OR Apache-2.0 | fs-dcx | deflate read and write |
| serde 1, toml 1 | MIT OR Apache-2.0 | fs-param | reading our own field layouts (`layouts/elden_ring.toml`) |
| clap 4 | MIT OR Apache-2.0 | fs-tool | the `apoc-fs` command line |
| tempfile 3 (dev only) | MIT OR Apache-2.0 | fs-dvdbnd tests | synthetic archives on disk |
| oozextract 0.5.7 | MIT label, **disputed** (port of GPL ooz) | fs-kraken only | Kraken decode. Reached only through `fs-dcx`'s `kraken` feature, which is off by default; `fs-tool` (a developer tool, never shipped) turns it on. Legal check before anything that ships enables it. |

`oozextract` also pulls in `bytes`, `futures`, `log`, `test-log`, `wide` and
`bytemuck` as normal dependencies, all MIT or MIT OR Apache-2.0.

Checked 2026-10-29 with `cargo tree --workspace -e normal,build`: every crate
in the tree is MIT, Apache-2.0, BSD-3-Clause, Zlib, Unicode-3.0 or Unlicense,
alone or in combination. Nothing is GPL, LGPL or MPL.

## Read as format references, not code

| Source | Licence | What was taken |
| --- | --- | --- |
| fstools-rs (soulsmods) | MIT + Apache-2.0 | Read for the archive layout (BHD5 entries, AES ranges, the 0x85 path hash) and its BND4 reader. No code copied; every fact was then checked against the game's files. It does not ship archive keys. |

## Facts that come from the game

- **Archive keys.** The five RSA public keys are read from the player's own
  `eldenring.exe`, where the game keeps them as PEM text, and matched to their
  archives by trying each. No key is stored in this repository.
- **The `regulation.bin` key.** The game keeps the AES-256 key as 32 plain
  bytes in `eldenring.exe`. `fs-param` finds it by trying each 4-byte-aligned
  window until one decrypts the file's first block to a DCX header, which takes
  about 0.2 s. No key is stored in this repository.
- **Param field layouts.** Worked out from the game's own tables with
  `apoc-fs columns`, never from Paramdex, and stored with the evidence for
  each field in `crates/fs-param/layouts/elden_ring.toml`.

## Do not use

SoulsFormats / SoulsFormatsNEXT, ooz, zao/ooz (GPL); Paramdex data (no
licence); fromsoftware-rs ("MIT NON-AI", not OSI); SoulsRandomizers (all
rights reserved).

## Not yet decided

- **Archive name list.** No list is used or committed. `apoc-fs check --names`
  counts any list it is given against the archives, but where a list comes
  from, and under what licence, is still open (PLAN.md §6).
