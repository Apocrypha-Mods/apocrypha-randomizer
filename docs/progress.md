# Progress

Phase gates from PLAN.md §5 and the evidence that each one passed.

| Phase | State | Evidence |
| --- | --- | --- |
| 0. Setup | done 2026-10-29 | Patch recorded, stock copy made, game boots unmodded and through me3 with an empty attach config (below). |
| 2. Formats | round trips done; fields in progress | Every PARAM (194 tables), FMG (3,968), EMEVD (589), MSB (1,347) and ESD (365) in the game round-trips byte for byte, and every parser has a mutation fuzz test. Field layouts and the instruction catalogue grow as later phases need them. |
| 1. Read the game's files | in progress | Every entry extracts and every DCX and BND4 round-trips (below). Open: the name-list count, and the step-3 boot test, saved for later. |
| 3. Game model | area map drafted; review in play | `apoc-fs model` finds 3,506 treasures, 133 one-time enemy drops, 2,169 gathering spots, 414 script awards, 112 NPC gifts, 191 boss rewards, 1,293 shop rows and 10 classes. `apoc-fs logic` binds the drafted area map (50 areas, 53 edges) with no unknown names, and the vanilla game walked from the start reaches every area and the Elden Beast. Open: the review of the 31 claims still marked, and the play-through review of the counts. |

## Phase 0: setup

### The game build we test against

Recorded 2026-10-29 from the install in `~/Drive/SteamLibrary` (Steam, Proton).

| Item | Value |
| --- | --- |
| Steam app / build id | 1245620 / 25080141 (installed 2026-10-08) |
| `eldenring.exe` FileVersion | 2.7.1.0 (exe resource) |
| In-game version (title screen) | App Ver. 1.17.2, Calibrations Ver. 1.17.3, with Shadow of the Erdtree |
| Base depot 1245621 | manifest 3886039026718227526 |
| Depot 1245624 | manifest 3497793843869306771 |
| Shadow of the Erdtree, depot 2778580 | manifest 1674424364022381183 (installed, `DLC.bhd/.bdt` present) |
| Oodle | `oo2core_6_win64.dll` |
| `regulation.bin` sha256 | `766521f9508de3a3532df61c45a1c2d93340f1ff7ed8306ab20df761712ca2ab` |

If Steam updates the game, these values change. Make a new stock copy for the new
build and keep the old one, so that tests can say which build they ran against.

### Stock copy

`~/Drive/GameStock/elden-ring/2.7.1.0-build25080141/` holds read-only copies of:

- `Data0`–`Data3` and `DLC` (`.bhd` + `.bdt`);
- `regulation.bin`, `oo2core_6_win64.dll` and `eldenring.exe`.

The sound archives (`sd/`) and movies are left out, since the randomizer never reads them.

- The copies are btrfs reflinks of the install, so they take no extra space. Steam
  rewriting the live files does not change them.
- `SHA256SUMS` in that folder was checked against the live install on
  2026-10-29, and every file matched.
- Re-check the copy with `sha256sum -c SHA256SUMS`.

Tools and tests read game files from this folder, never from the live install.
Nothing in it is committed.

### Boot checks

**Unmodded.** Launched from Steam on 2026-10-29 and reached the title screen,
which showed the version above and "OFFLINE".

**Empty me3 attach config.** Launched on 2026-10-29. me3's launcher and host
attached, and the game ran with no errors in the host log.

- me3 runtime v0.13.0 (`me3-launcher.exe`, `me3_mod_host.dll`), the copy the
  Apocrypha desktop app keeps in `~/.local/share/apocrypha/me3-runtime/v0.13.0/`.
  Its hashes match the app's `scripts/me3-runtime.lock`.
- Run through the game's own Proton build (`proton-cachyos-slr`, prefix
  CachyOS-11.0-100) with `proton waitforexitandrun me3-launcher.exe`. The launcher
  is told everything through `ME3_*` environment variables, each value JSON,
  as `apoc-me3/src/launch.rs` does.
- The attach config had no packages and no natives, and redirected the save to
  `apoc-rando-phase0.sl2`, so the real save was not touched. The host log shows
  that redirect taking effect, and no errors.

Two things that cost a failed launch each, worth knowing when we script this:

- The attach config must have every field of me3's `AttachConfig`, including
  an empty `[property_overrides]` table. If one is missing, the launcher exits
  with code 1 and logs nothing after "Parsed launcher args".
- The console file (`ME3_TELEMETRY_MONITOR_PIPE_PATH`) is opened without being
  created, so create it first. If it is missing, the launcher exits with code 1
  and writes no log at all.

## Phase 1: reading the game's files

### What was built

| Crate | Does |
| --- | --- |
| `fs-bytes` | Bounds-checked reads and the shared error type (from `apoc-fsparams`). |
| `fs-dvdbnd` | Reads the archive keys from `eldenring.exe`, decrypts and parses each `.bhd`, hashes paths, reads and decrypts entries from the `.bdt`. |
| `fs-dcx` | DCX read and write: zstd and deflate both ways, Kraken decode behind the `kraken` feature (off by default). Widened from `apoc-fsparams`. |
| `fs-kraken` | Kraken decoding through `oozextract`, kept on its own because that crate's licence is disputed. |
| `fs-bnd4` | BND4 read and write. The writer rebuilds names, the hash table and offsets rather than copying the original index. |
| `fs-tool` | `apoc-fs`, the developer tool: `survey`, `check` (the gate), `extract`, `rewrite`. |

Every parser has unit tests on synthetic files, including every truncation
and a repeatable mutation fuzz test (random byte damage must give an error,
never a panic). `crates/fs-tool/tests/real_files.rs` runs quick checks on
the real game when `APOC_ER_STOCK` names the stock copy, and skips otherwise.

### What the game's files showed

Everything below was checked against build 25080141 (App Ver. 1.17.2).

- **Archive keys:** `eldenring.exe` holds five 2048-bit RSA public keys as
  PEM text, one for each of `Data0`–`Data3` and `DLC`. Each `.bhd` decrypts
  with raw RSA, 256-byte blocks giving 255 bytes each.
- **Header salts:** `GR_other`, `GR_asset`, `GR_map`, `GR_chr` and `GR_sound`.
  The DLC archive's salt says "sound", and its byte at 0x05 is 0 rather than 1.
- **Encryption:** DLC entries are not encrypted at all. In `Data0`–`Data3` the
  encrypted ranges are AES-128-ECB, either the whole file or the first few
  kilobytes.
- **Entry counts:** 120,408 entries: `Data0` 5,878, `Data1` 39,419, `Data2`
  39,684, `Data3` 1,672, `DLC` 33,755. Every archive's last entry ends exactly
  where its `.bdt` ends.
- **Duplicate hashes:** 5 hashes are filed in more than one archive. The
  reader keeps the first archive's copy and lists the rest. Which copy the
  game prefers is not known.
- **DCX:** 103,286 files are Kraken (DCX version 0x11000, level 6) and 20 are
  deflate (version 0x10000, level 9). 19 of the deflate files store 0x24/0x2C
  in the header where the others store 0x44/0x4C, with the same physical
  layout. There is no zstd in the archives.
- **BND4:** every binder is little-endian with a hash table. Two format bytes
  are in use: `0x74` (ids, UTF-16 names, two size fields) and `0x70` (ids and
  single-byte ASCII names, two AutoInvadePoint lists in `Data0`). The name hash,
  bucket count and record order are in `crates/fs-bnd4/src/lib.rs`. Before the
  fix, 570 binders failed to round-trip for one reason: an empty file sits
  unaligned where the previous file ended.
- **Other formats:** the archives also hold 2,886 split binders (a BHF4 header
  file and a BDF4 data file in each pair), plus TPF, FLVER, MSB, EMEVD, FMG and
  others. BXF4 is not covered by this gate.

### Gate run, 2026-10-29

`apoc-fs check --jobs 14 ~/Drive/GameStock/elden-ring/2.7.1.0-build25080141`:

```
archives: 5  entries: 120408
entries extracted: 120408/120408
DCX decompressed: 103306  by compression: {"Deflate": 20, "Kraken": 103286}
DCX rewritten and round-tripped byte for byte: 103306/103306
deflate originals reproduced exactly by our encoder (informational): 0
BND4 round-tripped byte for byte: 74417 top-level, 3465 nested, holding 691738 files
failures: 0
time: 610.9s
```

- **Time:** 611 s on 16 cores from NVMe. Most of it is zstd level 9
  recompressing 110 GB.
- **Without recompression:** `--no-recompress` (extract, decompress and the
  BND4 round trip only) takes about 84 s.
- **Reading alone:** `apoc-fs survey`, which reads and decompresses every entry
  of 65 GB stored and 110 GB uncompressed, takes 78 s.

How "round-trip byte for byte" is checked:

- **DCX:** we cannot write Kraken, so each payload is rewritten as zstd (and
  as deflate when the original is deflate). The result must decompress to the
  same payload, and rewriting it again must give identical bytes.
- **Deflate originals:** our deflate encoder does not reproduce the game's 20
  deflate files exactly. That is expected, since the game's encoder settings
  are unknown, and the gate doesn't require it.
- **BND4:** each binder is parsed and written back, and the output must equal
  the original bytes, for every top-level and nested binder.

### Still to do

- [ ] **The name-list count.** The gate wants the extracted count to match a
      name list. No list has been chosen, because its origin and licence are
      open (PLAN.md §6). `apoc-fs check --names <file>` reports the coverage
      of any list it is given. One clean option is to build our own list from
      the game: names inside binders, path strings in the exe, and patterns
      such as map and character ids.
- [ ] **Step 3: the game accepts files we write.** Rewrite one map file and
      one event script as zstd or deflate and boot through me3. Saved for later
      on request; the code it needs (`Header::with_algorithm`, `compress`) is in
      place.
- [ ] **Legal opinion on `oozextract`** before anything that ships enables
      `fs-dcx/kraken`.

## Phase 2: formats

### PARAM

`fs-param` reads `regulation.bin` and writes it back:

- **The key** is found in the player's `eldenring.exe` (see
  `docs/dependencies.md`), so no key is stored here.
- **Each table** reads and writes back byte for byte.

The table code is lifted from `apoc-fsparams`. Its writer reproduced 138 of
the game's 194 tables; three changes, each found by comparing against the
game's file, bring that to 194:

- **The strings offset.** 56 tables store a strings offset that matches
  nothing, sometimes pointing past the end of the file. It is kept as read
  while the table's data is the same size.
- **Rows with no name.** A row whose name offset is 0 has no name, which is
  different from an empty name. When no row has a name, there is no names
  section at all.
- **Trailing bytes.** Five single-row tables have 8 zero bytes after their type
  name. These are kept under the same rule as the strings offset.

`apoc-fs params <game>` reports this; on 2026-10-29 it printed
`194/194 rewrite byte for byte`. The real-file test
`every_table_rewrites_byte_for_byte` holds it.

#### Field layouts

Paramdex has no licence, so every layout is worked out from the data with
`apoc-fs columns <game> <table> [--widths 421] [--where OFF=VALUE]`. For each
offset, read as i32, u16, u8 and f32, the command reports:

- how many distinct values there are, and how many rows are zero;
- the value range and the commonest values;
- which other tables' row ids the values fall in.

The layouts live in `crates/fs-param/layouts/elden_ring.toml`. Each field
records whether it is *confirmed* (the data settles it, and
`crates/fs-tool/tests/real_params.rs` re-checks it) or *probable* (it waits on
EMEVD, MSB or play), and the evidence either way.

| Table | Confirmed | Probable | How they were settled |
| --- | --- | --- | --- |
| `ItemLotParam_map`, `_enemy` | 8 × item id, category, weight, count; get-item flag | — | Item ids fall in the table their category names (92–100% per category). Slots 1–7 are empty together in all but 2 of 75,089 slots. Weights sum to exactly 1000 in 97–98% of rows. |
| `ShopLineupParam`, `_Recipe` | equip id, equip type, material set | price, two flags, stock | Equip ids fall in the table their type names, 97–100% for each type. Material sets resolve in 583 of 588 rows. |
| `NpcParam` | name, enemy item lot, map item lot | — | 213/223 names are NpcName ids; 98% and 100% of the lots are rows of those tables. |
| `CharaInitParam` | 4 × weapon, 4 × armor, 4 × ammo, 4 × talisman, 7 × spell, 19 goods slots in three runs | 2 × extra weapon | Every slot of the ten class rows (3000–3009) resolves. Armor slot i holds ids with hundreds digit i in all but 11 of 11,880 values. |
| `EquipParamGoods` | goods type | — | Read off the item names: 0 is other items, 1 crafting materials, 2 upgrade materials, 3 Great Runes, quest items and sorceries, 4 incantations, 6 letters and notes, 7 tutorials. |
| `EquipParamWeapon` | reinforce type | — | 32 of 33 values are rows of ReinforceParamWeapon, the start of an upgrade path. A path ending at +10 is somber (Moonveil: 2200); one ending at +25 is regular. |
| `GameAreaParam` | defeat flag | runes ×2, position | Round rune values on boss-like ids; the flags look like event flags; three floats. |
| `BonfireWarpParam` | grace name, sub category, entity id | found flag, position | 419 of 422 names are PlaceName ids, and 419 entity ids are assets placed in the maps. The flags are unique per grace, in 71000–76960. |
| `AssetEnvironmentGeometryParam` | pick-up lot, its prompt | break lot | Rows are asset models (99683 is AEG099_683). The pick-up lot is set in exactly the rows where the prompt (an ActionButtonParam row) is; 99% are ItemLotParam_map rows. A second lot column holds 15 values. |

Two developer commands support this:

- **`apoc-fs labels <game> <table> <names>`** lists, for each small-valued
  column, every value with the names of rows that have it. It names rows from
  the game's own English text, so a column's meaning can be read off directly.
  This is how goods type and reinforce type were settled.
- **`apoc-fs columns`** also tries base ids, the value with its last four
  digits cleared, so weapon ids with an affinity and a level still match.

The two layout tables encode an item's kind differently:

| Value | `ItemLotParam` category | `ShopLineupParam` type |
| --- | --- | --- |
| 0 | none | weapon |
| 1 | goods | protector |
| 2 | weapon | accessory |
| 3 | protector | goods |
| 4 | accessory | gem |
| 5 | gem | (one row) |
| 6 | custom weapon | |

`regulation.bin`'s own version stamp is `11711000` (1.17.1), while the game
reports App Ver. 1.17.2.

### Still to do in PARAM

- [ ] **`NpcParam` HP, runes and team.** The obvious candidates don't match
      bosses' known values: 0x24 for HP gives Rennala 1452 and 1703, and 0x2C
      for runes is mostly zero. Bosses such as Margit and Godrick have no row
      carrying their name. These are left for the enemy randomizer, where the
      spec and in-game checks can settle them, rather than recorded on weak
      evidence.
- [ ] **`SpEffectParam`.** The effects that scale enemies have no names to
      read them by. About a dozen `NpcParam` columns and five `EquipParamWeapon`
      columns point at it, and what each one means needs the enemy work, so it
      waits until then.
- [ ] **Promoting the probable fields to confirmed.** The item-lot flag is
      now confirmed, through MSB (below). Shop and grace flags are event flags,
      but what each records, along with prices, stock and rune rewards, waits
      on play.
- [ ] **No field is needed from `EquipParamProtector` or `EquipParamAccessory`
      yet.** An armor piece's slot is the hundreds digit of its id.

### FMG

`fs-fmg` reads and writes the game's text tables. All 3,968 FMGs in all 120,408
archive entries, every language included, round-trip byte for byte. `apoc-fs
check` now covers them.

Three things were found along the way:

- **Padding.** Each file is padded with zeros to a multiple of 4.
- **Extra zeros.** Some files carry more zeros than that. They are kept as
  read.
- **Leftover bytes.** Three files (`GR_NGWord` in zhoCN and
  `NetworkMessage_dlc01` in two languages) hold bytes between strings that no
  id points at. The writer does not rebuild those: it returns them as read
  while their entries are unchanged, and writes normally once an entry
  changes.

`apoc-fs check --no-recompress` on 2026-10-29:

```
entries extracted: 120408/120408
BND4 round-tripped byte for byte: 74417 top-level, 3465 nested, holding 691738 files
FMG round-tripped byte for byte: 3968
failures: 0
time: 95.2s
```

### EMEVD

`fs-emevd` reads and writes the event scripts. All 589 EMEVDs in the
archives round-trip byte for byte. `apoc-fs check` now covers them.

**Totals:** 4,908 events, 107,639 instructions, 34,483 parameters, 25
instruction banks.

The format matches the public description:

- **Layout:** a 0x90 header, then events (0x30 bytes each), instructions
  (0x20), layers, an argument blob padded to 16, parameters (0x20), linked
  files, and strings.
- **Arguments for an instruction with none.** The offset is 0 in 264 files and
  -1 in the other 325, and each file uses one or the other throughout, so it
  is kept per file.
- **Layers.** None of Elden Ring's 589 scripts uses them. They are handled and
  unit-tested, but nothing real exercises that code.
- **The string area** (names of linked files such as `common_func.emevd`) is
  kept as raw bytes, with linked-file offsets into it.

How the files are named: 544 of the 589 match the pattern
`/event/mAA_BB_CC_DD.emevd.dcx` or are one of the common scripts, which the
real-file test checks. The other 45 are named some other way, still unknown.

#### What the scripts confirm about the param flags

`apoc-fs flags <game>` looks for every flag-named layout field's values in
every argument word of every script:

| Field | In the scripts | Directly as a flag | Reading |
| --- | --- | --- | --- |
| `GameAreaParam.defeat_flag` | 206/207 | 163 | **Confirmed** as an event flag; the rest are passed to events as parameters. |
| `ShopLineupParam.event_flag_b` | 208/340 | 48 | Event flags; what each records waits on play. |
| `BonfireWarpParam.event_flag` | 155/422 | 32 | Event flags; what each records waits on play. |
| `ItemLotParam.get_item_flag` | 559/4,715 | 190 | Event flags. The low share fits pick-ups being set without a script; to be checked against MSB treasure. |
| `ShopLineupParam.event_flag_a` | 64/784 | 21 | Event flags; what each records waits on play. |

Where the boss-defeat flags fall points to the instructions that take a flag.
The instruction catalogue (below) records them. 4[0] isn't a flag instruction,
even though defeat flags show up in it: its word 1 holds character ids (10000
is the player), and boss area ids equal those character ids. 1003[1] was
first counted as one and then dropped, because its word 1 is mostly small
numbers (51–56).

#### Instruction catalogue

`crates/fs-emevd/catalogue/elden_ring.toml` records what instructions do, as
far as the game's scripts show it. Each entry has a plain name, a confidence
(confirmed or probable), evidence, and a kind for each argument word. No
third-party definitions are used.

How the evidence was gathered:

- **`apoc-fs instructions <game> [--only bank:id,…]`** reports every
  instruction's use count and argument lengths. For each argument word it
  shows the spread of values and which known sets they fall in: event ids,
  param flags, every text table, and every param table's row ids. It caches
  the 589 decompressed scripts in `scratch/emevd-cache` (not committed).
- **Placeholders are filled in first.** Many argument words are placeholders
  that parameters fill when the event starts.
  `fs_emevd::catalogue::resolve_calls` takes each `start_event` call (word 0 a
  slot, word 1 the event id, then the event's arguments) and copies each
  parameter's bytes from the call into the started event's instruction. That
  is 447,489 filled-in words. The reading is checked by the data:
  `award_item_lot_b`'s item-lot word goes from 45 to 620 distinct values and
  still matches ItemLotParam_map 95%, which a wrong reading would not do.

| Instruction | Name | Confidence | Evidence (shares of distinct values) |
| --- | --- | --- | --- |
| 2000[0] | `start_event` | confirmed | word 1 is an event id, 99% |
| 2000[6] | `start_common_event` | confirmed | word 1 is an event id, 99%, mostly common_func's |
| 2003[66] | `set_flag` | confirmed | 12 bytes: kind, flag, 0 or 1 |
| 3[0] | `if_flag` | confirmed | 8 bytes: packed condition, flag |
| 1003[2], 1003[101] | `branch_on_flag`, `_b` | probable | 8 bytes: packed bytes, flag; what they branch to is open |
| 2003[4] | `award_item_lot` | confirmed | ItemLotParam_map 100% |
| 2003[36] | `award_item_lot_b` | confirmed | ItemLotParam_map 95% of 619; how it differs is open |
| 1000[6] | `uses_item_lot` | probable | ItemLotParam_map 96% |
| 2004[76] | `item_lot_for_flag` | probable | a flag (207/207 used as one elsewhere; mostly boss defeat flags), then ItemLotParam_map 94% |
| 3[34] | `if_armor` | probable | EquipParamProtector 100% |
| 2003[11] | `boss_health_bar` | probable | on/off, boss id, bar 0–2, NpcName 100% |
| 2003[12] | `boss_defeated` | probable | GameAreaParam 95%, then a small number |
| 2004[8] | `sp_effect` | probable | a target, then SpEffectParam 99% |
| 2007[14] | `show_place_name` | probable | PlaceName 100% |
| 2007[15] | `show_tutorial` | probable | TutorialParam 100% |

`crates/fs-tool/tests/real_events.rs` re-checks every claim on the real
scripts:

- **Fixed lengths:** every use of an instruction has the length its entry
  states.
- **References:** at least 90% of a param argument's values, and 95% of a
  text argument's, are in their table.
- **Event ids:** at least 95% are ids of events in the scripts.
- **States:** a state word is only ever 0 or 1.

`apoc-fs flags` now takes its flag words from the catalogue.

### Still to do in EMEVD

- [ ] **Entity arguments:** boss ids are now checked against MSB (244 of the
      254 `boss_health_bar` bosses are enemy entity ids). spEffect targets are
      not checked yet.
- [ ] **The other 300 or so instructions** are uncatalogued. The ones Phase 5
      needs (enabling and disabling characters, warps, cutscenes) take
      entity ids, so they also come after MSB.
- [ ] **Names for the other 45 scripts.**

### MSB

`fs-msb` reads and writes the map files. All 1,347 MSBs in the archives
round-trip byte for byte, and `apoc-fs check` covers them.

**The container.** Six tables, always in this order: models, events, points,
routes, layers, parts. Each table has a version (74), an offset count that
includes a final next-table offset, its name (UTF-16, padded to 8), and its
entries.

**Entries are self-contained.** Every offset inside an entry is relative to
the entry's own start. So entries are kept as blobs, each running to where the
next one starts with its padding included, and only the tables are rebuilt.
Fields are edited in place inside the blobs. Adding or resizing entries (which
the enemy randomizer may need) will need the type-specific layouts first.

**Names.** 1,248 of the 1,347 maps match `/map/mapstudio/mAA_BB_CC_DD.msb.dcx`.
The other 99 are named some other way.

#### Field layout

Found with `apoc-fs msb-columns <game> <table> [--type OFF=V] [--base OFF]`.
`--base` measures from a section whose offset the entry stores. The results
are in `crates/fs-msb/layouts/elden_ring.toml`, and
`crates/fs-tool/tests/real_maps.rs` checks every claim against the other
formats:

| Field | Where | Checked by |
| --- | --- | --- |
| event / part type | +0x0C | Each type has its own data. Event type 4 is treasure. Part types 2 and 10 are enemies; 13 is assets (573,966 of them). |
| part model | +0x14 | The model's name is part of the part's name in 621,518 of 621,538 parts. |
| part position, rotation | +0x20, +0x2C | Float shape only (probable). |
| part entity id | section +0x60, +0x00 | 244 of the 254 boss ids the scripts give `boss_health_bar` are enemy entity ids. |
| enemy NpcParam, AI, CharaInit | section +0x68, +0x0C / +0x08 / +0x18 | 3,311/3,313, 1,350/1,352 and 183/183 distinct values are rows. |
| treasure part, item lot | section +0x20, +0x08 / +0x10 | 3,579 of 3,639 treasures point at a part (assets mostly); item lots are ItemLotParam_map rows (96%). |

#### What MSB settles

**The item-lot get flag is now confirmed** as the flag that records a treasure
as picked up:

- of the 3,558 lots the maps place as treasure, 3,425 carry a flag;
- only two of those flags are shared;
- only 12% of all get flags occur in any script.

So picking up the treasure sets the flag, not a script.

### Still to do in MSB

- [ ] **Points (regions) and routes** have no fields yet. The enemy
      randomizer's arena handling will want regions.
- [ ] **Entity group ids** (the words after the entity id).
- [ ] **Type-specific layouts** for when entries must be added or resized.
- [ ] **Names for the other 99 maps.**

### ESD

`fs-esd` reads and writes the talk scripts: the state machines behind NPC
dialogue, shops and gifts. All 365 ESDs in the archives round-trip byte for
byte, and `apoc-fs check` covers them. On 2026-10-29, `apoc-fs check
--no-recompress` reported `ESD round-tripped byte for byte: 365` and no
failures, in 102 s.

The layout was worked out from the files (see the module comment in
`crates/fs-esd/src/lib.rs`):

- **Header.** A 0x6C-byte header gives the size and count of each table.
- **Tables.** Fixed-size records follow one after another: the script
  header, state groups (machines), states, conditions, commands and command
  arguments.
- **After the tables:** a list of condition offsets, the expression bytes,
  and the name.
- **The writer** rebuilds the header from the tables. Rebuilding it right for
  every file is what confirms the layout.

**Where the scripts are.** 35 binders are named `/script/talk/mAA_BB_CC_DD`.
The open worlds' NPCs are in `m60_00_00_00` and `m61_00_00_00`, which have no
map file. A 36th binder, `m00_00_00_00`, holds the grace menu, Melina among
it; it was found by scanning the archives for ESDs outside the named
binders.

**Expressions** (`expr.rs`) are small stack programs. Only four readings are
needed so far, and `fs-tool/tests/real_talk.rs` checks them:

- each expression ends with 0xA1;
- 0x82 introduces a 4-byte integer;
- a byte below 0x80 is that byte minus 64;
- `[k, 0xB8]` is the k-th argument the machine was called with.

Of 100,374 arguments, 73,364 are plain numbers and 23,837 pass on an
argument.

**Calls** (`calls.rs`). A bank-6 command starts another machine of the same
script (all 28,528 name one), and which machine an id names differs between
scripts. So `resolve` follows every call from the machines nothing calls,
carrying numbers down, and reports the game's own commands with the values
they end up with.

**Commands** (`commands.rs`), checked by the real-file test:

| Command | Name | Evidence |
| --- | --- | --- |
| 1[104] | `AWARD_ITEM_LOT` | argument 0 is an ItemLotParam_map row in 122 of 128 distinct values |
| 1[22] | `OPEN_SHOP` | arguments 0 and 1 are the first and last ShopLineupParam rows of a range (100000–100024, …); 37 of 47 first rows are rows |
| 1[52] | (not catalogued) | (3, a goods id, -1): takes an item from the player (Deathroot to Gurranq) |

`1[144]` and `1[146]` also take shop-row ranges; they are not catalogued yet.

### Still to do in ESD

- [ ] **The other commands**, as later phases need them.
- [ ] **Conditions' evaluators** (which flags gate a gift) are not read yet.
      The logic will want them for quest gifts.

## Phase 3: game model

`crates/gamedata` extracts the model from the player's own files, and
`apoc-fs model <game> [--dump model.toml]` prints the coverage report the gate
asks for: locations by kind and by area, each area labelled with its graces'
names from the game's own text. The whole model (every location with its
items, every enemy, boss and grace) can be dumped as TOML for reading
through. `crates/gamedata/tests/real_model.rs` holds the counts and spot checks
below.

- **Model types** (`model.rs`) say nothing about Elden Ring.
- **The extraction** is in `elden_ring.rs`.
- **The sources** (`sources.rs`) are `regulation.bin`, every map and event
  script found by name, all 365 talk scripts, and the English item text.
- **Time:** everything is read and the model built in about 2.5 s.

### Where each kind comes from

| Kind | Source | Count | Flag |
| --- | --- | --- | --- |
| treasure | MSB treasure events: the part they sit on, the ItemLotParam_map row they give | 3,506 | 3,505 |
| enemy drop | a placed enemy's NpcParam lot (`_enemy` or `_map`) that carries a flag: drops once | 133 | all |
| repeat drop | the same with no flag: drops on every roll; counted once per map | 4,488 | none |
| gather | placed assets whose model's AssetEnvironmentGeometryParam row has a pick-up or break lot; counted once per map and lot | 2,169 | almost none: they grow back |
| event | lots the scripts award (`award_item_lot`, `_b`) | 414 | 413 |
| gift | lots the talk scripts award (`AWARD_ITEM_LOT`), calls followed | 112 | 87 |
| boss reward | lots awarded on a flag (`item_lot_for_flag`) | 191 | all |
| shop | ShopLineupParam rows; placed when every script that opens it is in one map | 1,293 (98 placed) | not read yet |
| starting | the ten classes, CharaInitParam 3000–3009 | 10 | — |

There are 31,969 enemies in the maps. There are 216 bosses (GameAreaParam
rows); 211 of them are placed, as the enemy whose entity id is the row id, and
212 are named from their health bars. There are 422 graces; 419 are placed, as
the asset whose entity id the grace row names, and they name the maps.

### What the extraction settled

- **Script arguments.** `fs_emevd::catalogue::uses` gives every use of an
  instruction with all its argument words, filled in from the calls that
  start the event, so pairs like a flag and a lot stay together. Calls are
  followed through shared events that start other shared events, though no
  extra lot turned up that way.
- **2004[76] takes a flag, not a boss entity**, and is renamed
  `item_lot_for_flag`. Its word 0 is used as a flag elsewhere in all 207 uses.
  In the map scripts it is the boss's defeat flag (the same number as the
  boss's entity id, which is why it was first read as an entity). The shared
  script `common` awards the remembrances and Great Runes on flags 9100–9270.
  Each of these is placed in the one map whose script sets the flag, which puts
  Godrick's Great Rune in Stormveil and Morgott's in Leyndell.
- **Lots run on into the next rows.** A lot is taken to include the rows that
  follow it directly, up to a row that something references itself.
  Godrick's lot (10010) gives his Great Rune, and the next row (10011) his
  Remembrance. Of the 63 treasure lots followed by such a row, 50 share its
  flag. This is probable and waits on play: 66 treasures and most repeat drops
  are chained this way.

### Gaps: what the extraction does not reach yet

When Phase 3 started, 902 ItemLotParam_map lots and 985 ItemLotParam_enemy
lots held items that nothing in the model gave. The talk scripts and
gathering bring the map lots down to 652. Sorted by whether the lot carries a
flag and whether its items have names:

| | items named | items unnamed |
| --- | --- | --- |
| flagged (one-time) | **293** | 16 |
| not flagged | 326 | 17 |

All 985 unreached enemy lots are unflagged, on NpcParam rows that no map
places. The 293 that matter fall in a few groups:

- **Numbered by map,** for example 1036430010, the Confessor set in m60_36_43,
  where the map's treasures are 1036430000, …20 and …30 (12xxxxxx in m12,
  21xxxxxx Rada Fruit in m21, 2049xxxxxx in the Land of Shadow). Neither the
  lot nor its flag is in any map we read, any script word, or any talk
  constant. **Checked in play (2026-10-29): the Confessor set is not found in
  m60_36_43.** It is only bought (the model has it in the Twin Maiden Husks'
  shop) or worn by the Confessor class (CharaInitParam 3006), and both match
  the model. So this lot is cut content, and the group as a whole is taken
  to be unused leftovers, not a source we are missing. The randomizer leaves
  them alone.
- **The 99 map files not found by name** hold 20 of them. Their parts are
  named `m463600_0000` and so on.
- **15 are numbered after an enemy's entity id** (564 enemy entity ids are
  lot rows, and 533 of those lots are already reached through scripts).
- **Low ids,** such as 40500–40666 (Smithing Stone [1], Golden Rune [1]) with
  flags 540500 and up, look like old test rows.

The groups below are what was found as the gaps were worked through:

- [x] **NPC dialogue gifts.** These were mostly the 100000s (164 of 189 of
      them flagged). Since `fs-esd`, 112 gifts are found: the grace menu gives
      the Spectral Steed Whistle and the Rold Medallion (`t000003000`), and
      Ranni, Hyetta and the others give theirs. 59 lots in the 100000s are
      still unreached, many in runs (101580–101583): perhaps the
      uncatalogued shop-range commands, or computed arguments.
- [x] **Gathering nodes.** These are reached through the asset's model:
      120 models with a lot are placed 18,056 times, and they give Golden
      Sunflowers, Knot Resin, Ghost Glovewort and so on. 199 more models carry
      a lot but are placed in no map we read; their items mostly have no
      name, so they look unused.
- [ ] **Tutorials** (the 50000s: "About Sites of Grace"). Their lots are not
      referenced; the game probably gives tutorials some other way.
- [ ] **Unflagged enemy lots on NpcParam rows that no map places.**
- [ ] **133 treasures whose lot is not a row**, and 54 whose lot holds no
      item.
- [ ] **The 99 maps and 45 scripts not found by name** (Phase 2) are not seen
      at all.
- [ ] **Shops.** The `OPEN_SHOP` ranges cover 550 of 1,296 rows. Most
      merchants are opened from several maps (the Twin Maiden Husks resell what
      bell bearings unlock), so only 98 rows get a single map. Which of the two
      shop flags records a purchase waits on play.

### The area map

`crates/logic` holds the area map: which areas connect, and what it takes to
get from one to the next. It is drafted by hand in
`crates/logic/data/elden_ring.toml` from general knowledge of the game
(PLAN.md §1, rule 4), and bound to the model, so every grace, item and boss
it names must be one the game has. `apoc-fs logic <game>` prints the binding,
the locations in each area, and a walk of the vanilla game from the start.

- **Areas** are sets of graces (by in-game name) and maps (by prefix).
  - An item location goes to its map's area, else to its nearest listed
    grace.
  - In the open worlds, "nearest" uses whole-world positions:
    `MapId::world` places a tile's local position on the world. Every one of
    the 213 open-world graces lands in a tile that exists, and 202 of them in
    the tile their entity id names.
  - Maps that hold two areas, such as Siofra and Nokron in `m12_02`, are
    split the same way.
- **Edges** open an area once their items are held and their bosses beaten.
  Leyndell's gate wants any two Great Runes.
- **Places** put gifts with no map in an area, optionally behind their own
  need. Ranni gives the Carian Inverted Statue for the Fingerslayer Blade, for
  example.
- **The walk** collects what each reached area gives and opens what that
  allows, until nothing changes.

On 2026-10-29 the draft had 50 areas, 53 edges and 5 places. The walk found
no unknown names, and the vanilla game reaches all 50 areas and the Elden
Beast. It also caught one mistake in the draft: Loretta's arena grace had
been put behind Loretta, which kept the Three Sisters and the Divine Tower of
Liurnia shut. `crates/logic/tests/real_graph.rs` holds the binding and the
walk, and checks that a gate holds when its key is taken out of the game.

#### Merchants

A shop row is sold wherever the characters stand whose talk scripts open a
range holding it:

- **Who runs a script.** An enemy part's `talk_id` (MSB, enemy section
  +0x10) is the talk script it runs. 318 of its 347 distinct values are
  scripts; `fs-tool/tests/real_maps.rs` checks it.
- **What opens a range.** Four talk commands open ranges of ShopLineupParam
  rows (`fs-esd/src/commands.rs`):
  - `OPEN_SHOP` 1[22], the merchants;
  - `OPEN_DRAGON_COMMUNION` 1[135], both churches and the Grand Altar;
  - `OPEN_PUPPET_SHOP` 1[144], Seluvis;
  - `OPEN_EXCHANGE` 1[146], Enia's and the Walking Mausoleum's remembrance
    lists (kind 1) and Seluvis again (kind 0).
- **Merchants who move** (Patches, D, the Blackguard, Corhyn, Moore, Ymir):
  the row gets every place they stand, as `also`, and the walk counts it as
  got at whichever is reached first.
- **The Twin Maiden Husks** resell merchants' stock once a bell bearing is
  handed over. Their copy of a row is dropped whenever someone else sells it,
  so that stock does not look buyable at the Roundtable from the start.
- **A script found standing nowhere** counts at its binder's map, after the
  places where its characters do stand.

That places 619 of the 1,293 rows. The area map's `shop_rows` entries deal
with the rest:

- 101500–101999 are Finger Reader Enia's (boss armour, and what remembrances
  trade for), so they go to the Roundtable Hold;
- 110000–112199 are altered garments and Ash of War duplication, which change
  items already held, so they are unused;
- a few lists no script opens (starter weapons, staves, dragon incantations
  at 600000, 900000, 1600100, 1600400, 9000000) are taken to be leftovers,
  marked for checking.

`crates/logic/tests/real_graph.rs` holds three things:

- every shop row has an area or is ruled out;
- the Crafting Kit is first placed where Kalé stands, in Limgrave;
- a location offered in two places is got at either.

After this, 560 locations are unused (559 shop rows and the Leyndell
medallion half). Only 24 are left "anywhere": 10 classes, 5 script awards
and 9 boss rewards.

#### To check in play

Every claim below is marked `sure = false` in the data file. Most of the
expansion is drafted with less confidence than the base game.

**Which area a grace is in:**

- Behind Caria Manor: with the Three Sisters, past Loretta?
- Great Waterfall Basin: in Nokron, rather than open Siofra?
- Deep Siofra Well: in Caelid, not Dragonbarrow?
- Capital Rampart: outside Leyndell's walls, on the plateau?
- Wyndham Catacombs and Craftsman's Shack: on Mt. Gelmir, not the plateau?
- Church of Repose: in the Mountaintops?
- The expansion's regions: Scadu Altus, Rauh, Hinterland, Cerulean Coast,
  Jagged Peak, Abyssal Woods, and which dungeons open off the Gravesite Plain.

**Settled in play (2026-10-29):**

- Stormveil to Liurnia does not need Godrick: a side path through the castle
  skips him.
- Leyndell to the Forbidden Lands needs Morgott.
- The Lake of Rot is open from the Ainsel River without Ranni's quest.
- The Rold Medallion is given by Melina in Morgott's arena, after him.
- Albus gives Haligtree Secret Medallion (Right), in the Village of the
  Albinaurics.
- The Gravesite Plain to Scadu Altus is open along the Ellac River, without
  Castle Ensis.
- The Pureblood Knight's Medal is from Varré's quest (Rose Church). It stays
  a key item the randomizer may put anywhere.
- Nobody in Leyndell gives Haligtree Secret Medallion (Left); it is only in
  Castle Sol. The Leyndell talk script's award (lot 102800) is marked
  `unused`. That is a new kind of entry: a location the game has but never
  gives in play, which the walk skips and the randomizer must not use.

**What a way needs:**

- Belurat to Enir-Ilim needs Messmer's Kindling?
- Smaller ones, each with a note in the file:
  - Limgrave → Roundtable Hold (Melina);
  - Stormveil → Divine Tower of Limgrave after Godrick;
  - Dragonbarrow → Divine Tower of Caelid;
  - Nokron → Deeproot Depths;
  - the academy's Abductor Virgin → Volcano Manor;
  - the Carian Inverted Statue → Divine Tower of Liurnia;
  - Lake of Rot → Moonlight Altar;
  - Leyndell → Divine Tower of West Altus;
  - Forbidden Lands → Divine Tower of East Altus → Isolated Divine Tower;
  - each of the expansion's edges.

### Still to do for the gate

- [ ] **Review the area map** (the list above) and correct the data file.
- [x] **Place the merchants** (above).
- [ ] **Shop requirements.** Enia's stock needs the boss beaten or the
      remembrance held, bell-bearing stock needs the bell bearing, and
      Dragon Communion needs Dragon Hearts. None of these is modelled yet.
- [ ] **The play-through review** of the coverage report.
- [ ] **The 293 flagged, named lots** are taken to be unused (one checked in
      play, above). Naming the 99 maps would account for 20 of them.
