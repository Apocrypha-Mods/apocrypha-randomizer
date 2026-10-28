# Elden Ring randomizer: formats research (clean side)

2026-10-29. No SoulsRandomizers material was read, and nothing in `scratch/rando-upstream` or `scratch/rando-spec` was opened.
How sure each claim is: **[code]** means read in source code. **[meta]** means taken from repo metadata or a licence file. **[belief]** means general community knowledge that I have not checked against bytes. **[verify]** means it needs the installed game.

## 1. Reading vanilla files on Linux without Oodle

**Archives.** `Data0-3.bhd/.bdt` plus the DLC pairs (`DLC.bhd/.bdt` and others) make up the DVDBND layer.
- Each `.bhd` header/index is **RSA-encrypted** with a per-archive public key. Decrypting it means raw RSA (modpow) over 256-byte blocks. [belief]
- fstools-rs ships those public keys under `keys/` (Apache-2.0/MIT). [meta]
- The index holds 64-bit **name hashes**, not names. ER's hash is a fold over the path: lowercase, `\`→`/`, a leading `/` is added, then `h = h*0x85 + ch` (u64 wrapping). [code: fstools-rs `crates/dvdbnd/src/name.rs`, Apache/MIT]
- Each entry gives an offset/size and possibly an AES-128 key with encrypted ranges, for partial AES-ECB of the BDT data. [belief, verify]

**The file-name list problem.**
- Hashes can't be reversed, so a reader needs a dictionary of known paths. Community lists exist (UXM Selective Unpack's dictionary, Nordgaren; its licence is not checked).
- For a randomizer this matters less than it seems. We only need a **small, known set of paths**: `regulation.bin`, `map/mapstudio/*.msb.dcx`, `event/*.emevd.dcx`, `msg/engus/*.msgbnd.dcx`, `script/talk/*.talkesdbnd.dcx`.
- We can hash the names we construct and look each one up. No full dictionary is needed. **Write our own list of required paths.**

**Compression (DCX).**
- `regulation.bin` is AES-256-CBC → DCX **ZSTD** (older patches DFLT). This is already proven in Nova's `apoc-fsparams`. [code]
- Most other ER game files (msb, emevd, msgbnd, talkesdbnd) are believed to be **DCX_KRAK** (Oodle Kraken). [belief, verify]
- fstools-rs `formats/src/dcx/` has `deflate.rs`, `zstd.rs` and `oodle.rs`. Its Oodle path goes through `crates/oodle-rt`, which uses `libloading` to load the game's `oo2core_*.dll` from the Steam install. **That cannot work natively on Linux or macOS**, because the DLL is a Windows PE. [code]

**Pure-code Kraken decoders**

| Project | Licence | State | Notes |
|---|---|---|---|
| powzix/ooz (C++) | **GPL-3.0-or-later** (file headers) | stale since 2019 | Must not be used or copied |
| zao/ooz (C++ fork) | no licence file detected; inherits GPL-3 | updated 2025 | Must not be used |
| lvlvllvlvllvlvl/oozextract (Rust, crates.io `oozextract` 0.5.7) | Cargo says **MIT**, but it describes itself as a "rust port of ooz" | active (pushed 2026-10-27) | **Licence risk**: a port of GPL code is likely a derivative work, so the MIT label may not hold. Fine for a local dev/verification tool; risky to ship in a proprietary binary. |

**Can `oo2core` be avoided?**
- Reading: only with a Kraken decoder. The options are:
  - (a) oozextract, which carries the licence risk above;
  - (b) a clean-room Kraken decoder written from a public format description. Little public spec exists, so this is high effort;
  - (c) on Linux, run the game's own DLL under Wine through a tiny helper `.exe` that decompresses on demand. The game is installed anyway and Proton is present, but this is clunky and doesn't help macOS;
  - (d) do the extraction once on a machine and cache decompressed vanilla data per game version. Distributing that cache is not allowed, but the user's own machine can hold it.
- **Recommendation:** start with (a) behind a feature flag in a separate crate, so it can be swapped out. Get a legal opinion before shipping. Keep (c) as a fallback.

**Writing.** No Kraken encoder is needed if outputs can be DFLT or ZSTD DCX, or uncompressed.
- The ER engine's DCX reader dispatches on the `DCP` algorithm tag, and the game itself loads ZSTD (regulation) and has historically loaded DFLT. That makes it very likely that `map/…msb.dcx` written as DCX_DFLT or ZSTD is accepted. Community tools commonly re-save ER files with non-Kraken compression. [belief, **verify** in game]
- me3 just redirects the path. It never touches compression. [belief, from Nova's own me3 notes in `apoc-me3`]
- **Riskiest unknown #1: does the game accept DFLT/ZSTD for every file kind we emit?**

## 2. Formats to read and write

Permissive references:
- **fstools-rs** (soulsmods; Apache-2.0 + MIT; last commit 2026-09-24; modules: bhd, bnd4, dcx, entryfilelist, flver, matbin, msb, param, tpf). Being Apache/MIT, it can be studied and depended on.
- **soulsmods.github.io / Soulsborne-Modding-Wiki**: format docs. No licence was detected, so read them for facts only and don't copy text.
- **SoulsFormats / SoulsFormatsNEXT**: GPL-3 per Nova's own notes, so don't read the code.
- **DSMapStudio**: MIT, C#. Its MSB/param editors are permissive references. Read it to understand structure; don't transliterate it.

| Format | Purpose | Structure | Permissive Rust |
|---|---|---|---|
| DCX | compression wrapper | 0x4C BE header, DCS/DCP/DCA, algorithm tag | apoc-fsparams (ZSTD/DFLT, read+write); fstools-rs (read) |
| BND4 | file container (regulation, msgbnd, talkesdbnd) | header, entries (flags, sizes, offsets, ids, UTF-16 names) | **apoc-fsparams `bnd4.rs`, read+write, round-trips on real files** |
| PARAM | data tables: ItemLotParam_map/enemy, ShopLineupParam, NpcParam, EquipParam*, CharaInitParam, … | header + row table (id, offset) + fixed-size row data | apoc-fsparams `param.rs` (rows as bytes); fstools-rs `param.rs` |
| Paramdef (layouts) | field names, offsets, types | — | see below |
| MSB | per-map placements: Parts (Enemy, Asset, Player…), Events (Treasure = item lot → asset), Regions, Models | params of typed entries with name/offset tables; ER is the MSBE variant | fstools-rs `msb.rs` + `msb/` (read; write unknown, check) |
| EMEVD | event scripts (flags, boss spawns, item awards, entity enable/disable) | events → instructions (bank, id, arg blob), parameter substitutions, linked files, string table | none found in Rust. Write our own from public docs. DarkScript3 is GPL? (unverified, so don't read it) |
| FMG | text (item names/descriptions, for the spoiler log/hints) | header, group ranges, string offsets, UTF-16 | none in Rust found; trivial format |
| ESD (talkesdbnd) | NPC talk/shop state machines; needed only if randomizing merchant-bound items | bytecode state machines | none; defer, use ShopLineupParam instead |
| Entity IDs / flags | MSB `EntityID` and `EntityGroupIDs` tie enemies to EMEVD; event flags gate items (ItemLot `getItemFlagId`) | convention: flag ranges per map block | data we derive ourselves |

**Paramdex.**
- soulsmods/Paramdex has **no licence**, so it is all rights reserved by default and can't be shipped or copied. [meta]
- Its README says the ER defs are "mostly official", i.e. derived from FromSoftware's paramdefs.
- paramdex-rs (MPL-2.0) is only a loader. It doesn't solve the data licence.

**Learning layouts from data.** Feasible, for the subset we need:
- Row size is known per param. Field boundaries and types can be inferred by comparing columns (constant/sparse, ID-like values that match other params' row ids, flag-like values in event-flag ranges, floats by bit pattern).
- The randomizer touches maybe 10 params and ~30 fields. **Recommend a hand-written, verified field map for only those fields**, built by observation and cross-referenced to MSB/EMEVD ids, and stored as our own TOML. This follows the clean method `apoc-fsparams` already used. [verify]

## 3. Nova's existing code (reusable, proprietary)

- `crates/apoc-fsparams`:
  - complete regulation pipeline: AES-256-CBC `crypto`, `dcx` (ZSTD/DFLT read+write, keeps the original header), `bnd4` read+write, and `param` with raw rows plus write;
  - word-level merge;
  - real-file tests (`tests/real_files.rs`) using the key from `apoc-gamedef/profiles/elden_ring.toml`.
  - **Directly reusable** as the PARAM/BND4/DCX base. Lift or depend on it, and generalise `dcx` to an algorithm trait so Kraken can plug in.
  - Note: its docs still say "MIT application". The desktop repo is now proprietary, so update that wording later.
- `crates/apoc-me3`:
  - builds a staged **overlay** and writes the attach config with one package pointing at it (`profile.rs`, `apply.rs`, `overlay.rs`);
  - `regmerge.rs` merges multiple `regulation.bin`s at deploy.
  - So a randomizer's output (a tree such as `regulation.bin`, `map/mapstudio/*.msb.dcx`, `event/*.emevd.dcx`, `msg/...`) becomes an ordinary mod in the overlay. **A randomized regulation must go through regmerge**, otherwise it would override other param mods.
- `crates/apoc-modengine` (`me3_profile.rs`, souls shapes): recognises loose-root mods. The ER roots (`event, map, msg, script, param…`) are verified in `elden_ring.toml`.
- Generator-tool flow (`apoc-domain/src/tool.rs` `ToolSpec`, `generator_cmds.rs`): this runs a **Windows** randomizer under umu/Proton, then zips its output and imports it as "<tool> — seed N", replacing the previous run in place.
  - A native randomizer can reuse the **Collect** half unchanged: write the output dir, zip it, `import_archive`, and replace in place with the seed in the name.
  - The Add/Run/.NET/Wine parts become unnecessary. Add a native ToolSpec variant, or call the library in-process.

## 4. Recommendation

Repo `apocrypha-randomizer` (Cargo workspace):
- `fs-dcx`: container + codec trait. `zstd` (MIT/Apache, wraps zstd BSD) and `flate2` (MIT/Apache) are built in; `kraken` sits behind a feature flag.
- `fs-kraken`: isolated decode backend (oozextract for now, licence flagged; swappable for a Wine-helper or clean-room decoder).
- `fs-dvdbnd`: BHD RSA decrypt (`rsa`/`num-bigint`, MIT/Apache, or hand-rolled modpow), name hashing, BDT reads with AES (`aes`, MIT/Apache).
- `fs-bnd4`, `fs-param` (from apoc-fsparams), `fs-msb`, `fs-emevd`, `fs-fmg`: read and write, with round-trip byte-identical tests.
- `er-data`: extracts vanilla data into a cached, version-keyed model, plus our own field maps (TOML).
- `rando-logic`: item graph (locations, key items, flag requirements) and enemy pools, seeded with `rand_chacha` (MIT/Apache).
- `rando-place`: writes modified param/msb/emevd and the spoiler log.
- `rando-cli`: `clap` (MIT/Apache). Later: `rando-host`, a lib/wasm API for Apocrypha's add-on host.
- Avoid: SoulsFormats(NEXT) and ooz (GPL), Paramdex data (unlicensed), fromsoftware-rs (custom "MIT NON-AI" licence, which adds restrictions and is not OSI).

**Build order (riskiest first):**
1. DVDBND read: RSA BHD, hash lookup, BDT extract. [verify on install]
2. Kraken decode: confirm oozextract output matches on real files, and settle the licence question.
3. Loose-override acceptance: write `msb.dcx`/`emevd.dcx` as DFLT/ZSTD and boot through me3. [verify in game]
4. PARAM field maps for ItemLot/Shop/NpcParam (learn from data).
5. MSB read+write round-trip (treasure events, enemy parts).
6. EMEVD read+write (item flags, boss/entity groups).
7. FMG (names for the spoiler log).
8. Logic, placement, CLI, then integration with apoc-me3 regmerge/Collect.

**Needs the installed game:** BHD key correctness for the current patch and DLC archives, the actual DCX algorithm per file kind, Kraken decoder parity, DFLT/ZSTD acceptance in game, field-map inference, and the MSB/EMEVD round-trip on real files.

Sources: github.com/soulsmods/fstools-rs, github.com/powzix/ooz, github.com/zao/ooz, github.com/lvlvllvlvllvlvl/oozextract, github.com/soulsmods/Paramdex, github.com/LuminaSapphira/paramdex-rs, github.com/vswarte/fromsoftware-rs, github.com/soulsmods/DSMapStudio, github.com/Nordgaren/UXM-Selective-Unpack.
