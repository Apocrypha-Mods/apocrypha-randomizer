# Progress

Phase gates from PLAN.md §5 and the evidence that each one passed.

| Phase | State | Evidence |
| --- | --- | --- |
| 0. Setup | done 2026-10-29 | Patch recorded, stock copy made, game boots unmodded and through me3 with an empty attach config (below). |
| 2. Formats | in progress | PARAM: all 194 tables round-trip, field layouts for 9 table types. FMG: all 3,968 round-trip. EMEVD and MSB not started. |
| 1. Read the game's files | in progress | Every entry extracts and every DCX and BND4 round-trips (below). Open: the name-list count, and the step-3 boot test, saved for later. |

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
| `ItemLotParam_map`, `_enemy` | 8 × item id, category, weight, count | get-item flag | Item ids fall in the table their category names (92–100% per category). Slots 1–7 are empty together in all but 2 of 75,089 slots. Weights sum to exactly 1000 in 97–98% of rows. |
| `ShopLineupParam`, `_Recipe` | equip id, equip type, material set | price, two flags, stock | Equip ids fall in the table their type names, 97–100% for each type. Material sets resolve in 583 of 588 rows. |
| `NpcParam` | name, enemy item lot, map item lot | — | 213/223 names are NpcName ids; 98% and 100% of the lots are rows of those tables. |
| `CharaInitParam` | 4 × weapon, 4 × armor, 4 × ammo, 4 × talisman, 7 × spell, 19 goods slots in three runs | 2 × extra weapon | Every slot of the ten class rows (3000–3009) resolves. Armor slot i holds ids with hundreds digit i in all but 11 of 11,880 values. |
| `EquipParamGoods` | goods type | — | Read off the item names: 0 is other items, 1 crafting materials, 2 upgrade materials, 3 Great Runes, quest items and sorceries, 4 incantations, 6 letters and notes, 7 tutorials. |
| `EquipParamWeapon` | reinforce type | — | 32 of 33 values are rows of ReinforceParamWeapon, the start of an upgrade path. A path ending at +10 is somber (Moonveil: 2200); one ending at +25 is regular. |
| `GameAreaParam` | — | runes ×2, defeat flag, position | Round rune values on boss-like ids; the flags look like event flags; three floats. |
| `BonfireWarpParam` | grace name, sub category | found flag, entity id, position | 419 of 422 names are PlaceName ids. The flags are unique per grace, in 71000–76960. |

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
- [ ] **Promoting the probable fields to confirmed.** Item-lot, shop,
      boss-defeat and grace flags wait on EMEVD and MSB. Prices, stock and rune
      rewards wait on play.
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
