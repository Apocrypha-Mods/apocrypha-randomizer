# Progress

Phase gates from PLAN.md §5 and the evidence that each one passed.

| Phase | State | Evidence |
| --- | --- | --- |
| 0. Setup | done 2026-10-29 | Patch recorded, stock copy made, game boots unmodded and through me3 with an empty attach config (below). |
| 2. Formats | round trips done; fields in progress | Every PARAM (194 tables), FMG (3,968), EMEVD (589), MSB (1,347) and ESD (365) in the game round-trips byte for byte, and every parser has a mutation fuzz test. Field layouts and the instruction catalogue grow as later phases need them. |
| 1. Read the game's files | in progress | Every entry extracts and every DCX and BND4 round-trips (below), and the game runs on files we write (step 3, 2026-10-29). Open: the name-list count. |
| 3. Game model | area map drafted; review in play | `apoc-fs model` finds 3,506 treasures, 133 one-time enemy drops, 2,169 gathering spots, 414 script awards, 112 NPC gifts, 191 boss rewards, 1,293 shop rows and 10 classes. `apoc-fs logic` binds the drafted area map (50 areas, 53 edges) with no unknown names, and the vanilla game walked from the start reaches every area and the Elden Beast. Shop rows wait for their bosses, bell bearings, handed-over books and currencies; 58 rows behind 26 unexplained flags are not counted. Gifts wait for what their talk asks, and for the quest stages the events set: 29 of the 86 that waited on stages are counted, and 57 still wait on a condition not yet read. 87 quest gifts may be missed and take no key item. Open: the review of the 31 claims still marked, and the play-through review of the counts. |
| 4. Item randomizer | done 2026-10-29 | `apoc-rando check` placed and proved 10,000 seeds (0 to 9,999) finishable in 526 s on four threads, none failing. Seed 1 booted in the lab, and the chapel's first pick-up gave the Twinned Helm its spoiler log names (below). Open: the options the spec will define. |
| 5. Enemy randomizer | started 2026-10-30; not seen in game | `apoc-rando gen --enemies` writes a seed, and the real-file test reads it back. No lab run was made, at Nova's request (below). Its gate, in-game hazard checks, is open. |
| 6. Output and merging | done 2026-10-31 | A seed on Nightreign Movement and ELDEN VINS together ran in the lab: the seed's files served over both, the world loaded, and a moved boss fought under its own name (below). |
| 7. Cross-platform | small CI 2026-10-31; release later | `.github/workflows/ci.yml` builds and tests on Linux, Windows and macOS, and lints on Linux. Tests that need the game skip there. By Nova's choice the byte-identical check and packaging wait until there is something to ship, after the add-on. |

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

### Patch 2.7.2 (build 25827938)

Steam moved the game on 2026-10-29. Tools and tests now read
`~/Drive/GameStock/elden-ring/2.7.2.0-build25827938/`, made the same way; the
old copy is kept. Depots 1245621 and 1245624 have manifests
1925130620768035844 and 4777545061760251630; the DLC's is unchanged.

| What | Change |
| --- | --- |
| `eldenring.exe` | 2.7.1.0 to 2.7.2.0 |
| `regulation.bin` | version 11711000 to 11730034, sha256 `b8cdab84c0772f66811fdc5176a381877ca6657b37d420cc2c1562a90d63870e` |
| Tables | `apoc-fs param-diff`: one row of `Bullet`, one byte; nothing the randomizer reads |
| Archives | `Data0` and `Data3` differ; `Data1`, `Data2` and `DLC` are the same. Which entries changed was not looked at |
| Game model | `apoc-fs model --dump` and `apoc-fs logic` print the same on both |

Rerun on the new copy, 2026-10-30:
- `apoc-fs check --no-recompress`: 120,408 entries, no failures, in 117 s;
- `cargo test --release --workspace --exclude native`, real-file tests included:
  all pass;
- `apoc-rando check --seeds 10000`, four threads: none failing, in 1,260 s
  beside two Cyberpunk labs.

`apoc-rando` now refuses a `regulation.bin` whose version it has not been
checked on (`KNOWN_REGULATIONS`), for both `gen` and `check`. A patch that left
the stamp alone would get past this.

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

### Step 3: the game runs on files we write

`apoc-fs boot-test <game> <out>` builds a me3 mod folder, every file written
by our own code and compressed with zstd:
- `regulation.bin`, with the Vagabond starting with Moonveil;
- `msg/engus/item_dlc02.msgbnd.dcx`, with Moonveil renamed "Moonveil
  (Apocrypha boot test)";
- the maps and event scripts of the Chapel of Anticipation (`m10_01_00_00`)
  and the Stranded Graveyard (`m18_00_00_00`), rewritten unchanged.

Run on 2026-10-29 in the headless lab (`tools/er-lab`, below):
- me3 served all four of the files the run reached (the regulation, the item
  text, and the chapel's map and events);
- the class screen showed the Vagabond holding a katana;
- the equipment screen read "Moonveil (Apocrypha boot test)";
- the chapel loaded from our map and event script, and the character walked
  through it with the HUD showing Moonveil.

### The headless lab

`tools/er-lab` runs the game where it cannot disturb the desktop, for checks
like this one and Phase 4's "one seed boots":
- **Display:** headless gamescope at 640x360 and 10 fps.
- **Its own prefix** (`scratch/lab`), with low graphics and a copy of the save.
- **Limits:** nice 19, idle I/O, a 400% CPU quota and a 9 GB memory cap, in
  the `apoc-er-lab` scope.
- **Audio:** a null sink, also muted.
- **Input:** keys go to gamescope's own X display (`keys.sh`), and
  `gamescopectl screenshot` takes the pictures.

Getting it to run took four findings:
- **Steam must be running.** Without it `SteamAPI_Init` fails with "create
  pipe failed", and the game quits.
- **Pin no cores.** With `taskset`, Wwise cannot pin its threads and sound
  fails to start, which stops the game (`MOWwiseMan.cpp(883)`), so the CPU is
  limited by quota instead.
- **me3's launcher check.** Its Steam check fails inside gamescope, so the
  attach config sets `skip_steam_init`.
- **`ME3_GAME_LAUNCHED`.** me3's host panics without it ("ME3_GAME not set"),
  alongside the `ME3_LAUNCHER_*` variables, `ME3_LAUNCHER_ARGV` among them.

A first attempt ran the game full screen beside other games and the PC
froze. The lab exists so that does not happen again. Check the machine's
load before a run.

### Still to do

- [x] **The name-list count.** `apoc-fs names` builds our own list from the game:
      names inside binders, path strings in the exe and in files, id patterns, map
      ids and words in known folders, each kept only if its hash matches an entry.
      On 2.7.2.0 it names 116,598 of 120,408 entries (96.8%) in about 35 minutes.
      Of the 3,810 left, 3,674 are NVMC files in `Data2` and `DLC`, and 136 are
      others. The list is game data, so it stays in `scratch/`. It has not been run
      through `apoc-fs check --names`, and it does not reach the count the gate
      wanted, which was a match with a whole list.
- [x] **Step 3: the game accepts files we write** (below).
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
| `ShopLineupParam`, `_Recipe` | equip id, equip type, material set | price, on-sale flag, the other flag, stock, currency | Equip ids fall in the table their type names, 97–100% for each type. Material sets resolve in 583 of 588 rows. |
| `NpcParam` | name, enemy item lot, map item lot | — | 213/223 names are NpcName ids; 98% and 100% of the lots are rows of those tables. |
| `CharaInitParam` | 4 × weapon, 4 × armor, 4 × ammo, 4 × talisman, 7 × spell, 19 goods slots in three runs | 2 × extra weapon | Every slot of the ten class rows (3000–3009) resolves. Armor slot i holds ids with hundreds digit i in all but 11 of 11,880 values. |
| `EquipParamGoods` | goods type | — | Read off the item names: 0 is other items, 1 crafting materials, 2 upgrade materials, 3 Great Runes, quest items and sorceries, 4 incantations, 6 letters and notes, 7 tutorials. |
| `EquipParamWeapon` | reinforce type | — | 32 of 33 values are rows of ReinforceParamWeapon, the start of an upgrade path. A path ending at +10 is somber (Moonveil: 2200); one ending at +25 is regular. |
| `GameAreaParam` | defeat flag | runes ×2, position | Round rune values on boss-like ids; the flags look like event flags; three floats. |
| `BonfireWarpParam` | grace name, sub category, entity id | found flag, position | 419 of 422 names are PlaceName ids, and 419 entity ids are assets placed in the maps. The flags are unique per grace, in 71000–76960. |
| `EquipMtrlSetParam` | 6 × item id, category | 6 × count | Category 4 ids are EquipParamGoods rows (93% in slot 0), category 1 ids EquipParamProtector rows (99%); filled in 831, 165, 97, 24, 34 and 1 rows. |
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
| 1003[2] | `end_if_flag` | probable | 8 bytes: end, state, flag kind; then a flag. First catalogued as `branch_on_flag` |
| 1003[101] | `goto_if_flag` | confirmed | a label later in the event in 2,112 of 2,128 uses. First `branch_on_flag_b` |
| 1003[1], 1003[3] | `skip_if_flag`, `skip_if_flag_range` | confirmed, confirmed from the exe | never skip past the event's end |
| 1003[12–14] | `skip_if`, `goto_if`, `end_if` | probable | test one unnamed thing, 0 in a world of one's own |
| 0[0] | `combine_group` | probable | puts one group's state into another, or waits on it |
| 1000[1–4], 1000[101], [103], [107] | skips, ends, gotos on condition groups | confirmed, probable | skip counts fit, labels lie ahead |
| 1001[0], [1], [2], [6] | waits | probable | a float in seconds, or a small count |
| 3[1] | `if_flag_range` | probable | state 2 is any of the range on |
| 2003[63], 2003[69] | `set_flag_range`, `set_flag_b` | probable | clear a stage range; set like `set_flag` |

**Control flow.** Every condition instruction (banks 0 to 999) puts its test
in a condition group, byte 0 of its first word, always -15 to 15. Group 0 is
the event waiting. A negative group holds when any of its conditions does, a
positive one when all do: quest stages are cleared as a range and one is then
set, so two stages of one range are never on together, and 98 of 597
negative groups ask for two such stages against 10 of 1,144 positive ones.
`every_packed_byte_claim_holds` re-checks each byte the catalogue names.
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
- [ ] **The conditions quest events wait on** (3[3], 3[12], 4[0], 4[5] and
      others): read, they would open more of the 54 gifts still shut.
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
| 1[11] | `SET_EVENT_FLAG` | (flag, 1): Sellen's script sets 1044369244, the on-sale flag of Glintstone Cometshard |
| 1[52] | `TAKE_ITEM` | (3, a goods id, -1): takes an item from the player (the Conspectus Scroll to Sellen) |

**Handovers** (`handover.rs`). A state that takes one item, and the flags
turned on in the states that lead into it or follow it with no choice
between, pair the item with the flags. Sellen takes the scroll and then sets
the flag; Corhyn sets the flag and then takes the prayerbook. Every scroll and
prayerbook the two take pairs this way. Items and flags passed
through a call are not followed, which leaves Miriel's out.

**Conditions** (`cond.rs`). An expression parses as a stack program:
- calls (0x84–0x8A, the function's number first);
- arithmetic, comparisons, and/or;
- register stores (0xA6–0xAD) and loads (0xAF–0xB6);
- the machine's own values (0xB9, 0xBA);
- floats and doubles.

116,769 of the 116,771 conditions in the game parse to one value; the two
left use 0xAE. Two functions matter:
- **Function 15** is the event-flag test.
- **Function 47** compares how many of an item the player holds: (type, id,
  comparison, count, …), where `(3, id, 4, 1, 0)` means "holds at least one".

What a condition asks for is read from the parts joined by "and" (6,491
conditions ask for a flag or an item).

**Reach** (`reach.rs`). Within a machine, what holds on every way from its
first state to each state. `calls::resolve` now carries this down each chain
of calls, so every command it reports comes with what holds where it runs.

`1[144]` and `1[146]` also take shop-row ranges; they are not catalogued yet.

### Still to do in ESD

- [ ] **The other commands**, as later phases need them.
- [x] **Conditions' evaluators** for flags and items held (above).
- [ ] **Register 8** (0xAE), in two conditions.
- [ ] **Following items through calls** in handovers (Miriel).

## Phase 4: item randomizer

`crates/items` places items and `crates/rando` is the command line,
`apoc-rando`. No behaviour spec has arrived yet (`docs/spec/` holds only its
README), so this is the core PLAN.md's gate asks for, with no options beyond
the seed.

- **What moves.** The unit is one item slot of an item-lot row: 4,722 slots
  of treasure, one-time enemy drops, script awards, NPC gifts and boss
  rewards. The pick-up flag and the slot's chance stay; only the item, its
  kind and its count move. Shops, gathering, farmable drops and starting
  gear stay as they are.
- **Key items** are every item a requirement names: the area map's edges,
  places, goal and `on_sale` entries, shop costs and currencies, and what
  gifts ask to be held. 165 slots hold one. They are placed first, by
  assumed fill: each goes in a random slot that a walk reaches while
  assuming every key item not yet placed is held. A slot may take a key item
  only if it always gives its item and its location is in an area.
- **Everything else** is shuffled into the slots left.
- **The proof.** The finished placement is walked with nothing assumed, and
  the Elden Beast must be reached. A seed that corners itself is retried, up
  to 20 times, from the same generator.
- **Seeds.** xoshiro256** seeded through SplitMix64, written here from the
  published algorithms and pinned by a test against values computed
  separately; every collection is walked in a fixed order.
- **Output.** `apoc-rando gen <game> --seed N --out DIR` writes `regulation.bin`
  into a me3 mod folder and the spoiler log beside it: where each key item
  went, then every slot by area, with what it held before.

**The player's sliders** (docs/spec/items.md §2.1, §2.3), from
`apoc-rando --shuffle <category>=<percent>,… --bias <category>=<percent>,…`:
- **Two sliders per category.** Each of the ten categories (key items,
  weapons, armour, talismans, spells, Ashes of War, spirit ashes, upgrades,
  quest items, consumables) has a shuffle slider, how much of it moves, and
  a bias slider, how strongly what moves leans deep.
- **Categories on the real game:** 165 key items, 439 weapons, 396 armour,
  153 talismans, 122 spells, 86 Ashes of War, 102 spirit ashes, 769
  upgrades, 449 quest items and 2,041 consumables.
- **Depth** (`crates/items/src/depth.rs`) is our own: three quarters how late
  the vanilla walk opens the slot's area, a quarter how hard the slot is.
- **The effect of bias:** at full key bias, key items sit at a mean depth of
  0.72, against 0.47 with none (10 seeds each).
- **Weights are whole numbers** from our own `exp`, built from plain
  arithmetic, so every platform draws the same.
- **Every option goes into the spoiler log.**
- **Merchants and starting gear** are slots too, each with its own shuffle
  slider: 661 shop rows paid for in runes (a row keeps its price and stock)
  and 68 starting slots of the ten classes (weapons, armour, talismans,
  spells). [`fits`] keeps each slot to items it can take:
  - no weapon with a set affinity in a shop;
  - armour for the same body part;
  - no arrows in a hand;
  - no key item in a class's hands.

  Gate rerun with them, 2026-10-29: 10,000 default seeds in 1,120 s, 2,000
  at full bias in 222 s, and 2,000 with merchants and starting gear held
  still in 164 s, none failing. These ran on three threads beside the
  Cyberpunk lab, hence the time.

  The starting slots are filled first, after the key items.
- **Quest items** (spec §2.4, §4). `Model::quest_items` holds the 103 items
  talk scripts take from the player or ask them to hold, read from `TAKE_ITEM`
  and from what holds along each call chain. Before this, a quest item that no
  requirement named was shuffled like anything else: over 20 seeds, 194 of
  4,940 landings were unsafe: 91 on missable gifts, 67 on gifts the walk never
  counts and 36 on locations in no area. Irina's Letter could go there, and
  Edgar's quest with it.
  - One copy of each, unless a copy stays where the unmodded game has it, goes
    on a slot that always gives it, on a non-missable location the finished
    walk reaches. Its own category's bias still applies. 73 of the 103 are not
    key items; the rest are placed as key items, with the same rule.
  - No quest item goes past the burning of the Erdtree (`no_return`: Farum
    Azula, the Ashen Capital, the Elden Throne). This is cautious and not
    checked quest by quest; it only costs choice.
  - `check` now proves all of it, and fails a seed that breaks it.
  - **What this does not prove:** that a quest's own steps can be done in the
    order it asks. The logic reads no quest as a whole, only that its item can
    be had; whether each quest finishes wants play.
  - Gate rerun, 2026-10-30, none failing: 10,000 default seeds in 585 s, 2,000
    at full bias in 119 s, and 2,000 with the DLC separate, half of everything
    moving and keys at 60% bias in 78 s.
- **Base game and DLC** (`--dlc mixed|separate`). The DLC is told by map:
  m20–m22, m25, m28, m40–m43 and m61. That gives 1,371 of the 5,451 slots.
  Separate, no item crosses sides. 2,000 seeds proven, none failing, in
  226 s beside the Cyberpunk lab. With shops in
  play, key items average a depth of 0.42 at no bias, as merchants stand
  early, and 0.70 at full bias.
- **Gate rerun, 2026-10-29** (nice 19, four threads), none failing:
  - 10,000 seeds with the default sliders, in 567 s;
  - 2,000 with every bias at 100%, in 113 s;
  - 2,000 with half of everything moving and keys at 60% bias, in 64 s.

Making the walk fast enough mattered: rounds now visit only the locations
that give something or that another waits on, and the rest are taken in one
pass at the end. Taking a location now also counts as a change, since a gift
may wait on another location being got.

- **The chapel's door** (found in play, 2026-10-30). Seed 1 moved the
  Tarnished's Wizened Finger out of the chapel, the door there opens only to
  those carrying it, and the character was shut in. The area map starts at the
  Stranded Graveyard, past that door, so the walk never saw it. The finger is
  now `fixed` in the area map and `check` fails a seed that moves it. Gate
  rerun on 2.7.2, four threads, beside the lab and the game: 10,000 seeds in
  1152 s, none failing.
- **`apoc-fs test-kit <game> <mod>`** gives a new character 50 of each smithing
  stone and Numen's Rune in the chapel, for testing upgrades by hand. Seen
  arriving in the lab on 2.7.2. `tools/er-lab/play.sh` runs a mod folder on the
  desktop. It ran on 2026-10-30, and the door above is what that found.

- **From play, 2026-10-30** (spec §2.6a to c):
  - Seen working in a real playthrough: auto-upgrade gave a new weapon at +3
    after one was raised to +3.
  - `fixed` now also holds the three flasks, the whistle and the Crafting Kit.
  - **Keepsakes** are CharaInitParam 2401–2409, a new `Kind::Keepsake`; 2400 is
    "None". Their counts are at +0xCC (`goods_count`). Character creation names
    them from GR_MenuText 297151–297160, not from the item, and the game reads
    `menu_dlc02.msgbnd.dcx`, not `menu.msgbnd.dcx`. In the lab the menu listed
    seed 1's nine. A chosen keepsake arriving, and its count, are not yet seen.
  - **Gathering**: 169 more slots, in a pool of their own.
  - **NPC equipment** (`dress_npcs`): 182 CharaInitParam rows placed in the
    maps, 776 changes on seed 1. Seen on Varré in the lab (below).
  - Gate on 2.7.2, four threads, beside the lab: 10,000 seeds in 1,155 s, none
    failing.

Two things to know when reading a spoiler log:
- Some key items exist twice in the game's lots, such as Godrick's Great
  Rune and the Academy Glintstone Key, and both copies are placed.
- Dragon Hearts count as key items, as Dragon Communion is paid in them.

**Gate run, 2026-10-29.**
- `apoc-rando check <stock> --seeds 10000` (nice 19, four threads) placed and
  proved seeds 0 to 9,999 in 526 s, with none failing.
- **Seen in the lab, seed 1, a new character** (2026-10-30, game 2.7.2):
  - The keepsake: choosing Smithing Stone [6] gave item 10105, count 1. Only
    this one was tried, so no talisman keepsake and no count above 1.
  - NPC equipment: Varré at The First Step wears the Glintstone Scarab and a
    pale tabard over bare legs, not the white mask and robe.
  - Gathering: a plant by The First Step gave Smithing Stone [5], one of the
    seed's drops for the Rowa Fruit node, and the count in the inventory rose.
  - Kalé's shop was not visited; the Crafting Kit rests on the spoiler log and
    the tests.
  - Lab tools from this: `apoc-fs test-warp` lands the player elsewhere on
    loading the chapel (2003[14], the map's four bytes then an entity), and
    `APOC_NATIVE_INVENTORY=1` logs every inventory record. In the lab's menus
    Escape closes help panels; no key or click found changes inventory tab. A
    brand-new save stops at the publisher's privacy policy, so use the
    existing lab save.
- Seed 1 then ran in the headless lab (`ER_MOD=scratch/seed1
  tools/er-lab/run.sh`, the first run of the repo copy of the scripts). me3
  served its `regulation.bin`, the save loaded in the Chapel of Anticipation,
  and the body by the start, which gives the Tarnished's Wizened Finger in the
  unmodded game, gave the Twinned Helm, as the spoiler log says.

`crates/items/tests/real_items.rs` places 20 seeds on the real game and checks
that a plan is written into ItemLotParam_map slot by slot; the unit tests
check, on a small made-up map, that the key never lands behind its own door,
that a seed always gives the same plan with every item placed once, and that
a key with nowhere to go fails the seed.

- **Languages** (2026-10-31). Keepsake names and health bars are written in all 15 languages the game
  ships, each from its own item and name text, English where it has none. The keepsake menu's one line
  of help is English in all. Seed 7 with bosses moved adds 28 text files and 16 MB in all; its other files
  and spoiler log are byte for byte those of the build before. Other languages have not been seen in the game.

### The native module (auto-upgrade)

The spec's auto-upgrade (`docs/spec/items.md` §2.7) depends on what the player
does during play, so it lives in a module of ours inside the game:
`crates/native` builds `apoc_native.dll` for Windows (`cargo build --release
-p native --target x86_64-pc-windows-gnu`, mingw), and me3 loads it as a
native next to the mod folder (`ER_NATIVE=<dll> tools/er-lab/run.sh`).

- On 2026-10-29 it loaded in the lab: its log read `apoc_native 0.1.0 loaded
  into process 376`.
- me3's `Native` entry takes `path`, `optional`, `load_before`, `load_after`
  and `load_early`. Its initializer and finalizer can be left out.
- **Auto-upgrade works in the lab** (2026-10-30, game 2.7.2). How it works:
  it changes no game code, only table values. Every two seconds it reads the
  player's weapons. When the highest raised level on a path changes, it
  rewrites the item id of every weapon pick-up (1,088 lot slots and rune-shop
  rows) to the weapon's base id plus that level. The game then makes the
  weapon at that level as it gives it.
- **The lab test.** The chapel's event script was given three extra
  instructions: award lot 103500 (patched to give custom weapon 5010,
  Rogier's Rapier +8), wait 10 s (1001[0], one float in seconds, used 1,883
  times in the game's own scripts), then award lot 10010000, seed 22's
  Twinblade. In that order:
  - the module's log showed the Rapier arrive (5030008);
  - it read +8 regular and rewrote the pick-ups (15 ms);
  - the Twinblade arrived as 10000008, Twinblade +8.
  
  With no raised weapon held, the same award gave a plain Twinblade. The
  game's memory stayed at 5.3 GB.
- **Three copies of the regulation** sit in memory, identical. Writing +1, +2
  and +3 into them gave Twinblade+3, so the game gives from one copy only.
  The param store (`CS::ParamRepositoryImp`) points at that copy's tables
  directly, five pointers in, and at no other copy.
- **Never read all memory.** A first version scanned every writable region
  for the tables and the weapon instances. Reading pages the game reserved
  but never touched makes the system back them, and a run that scanned twice
  pushed the game past the lab's 9 GB cap. The module now follows the game's
  own pointers (`crates/native/src/walk.rs`):
  - the game image holds pointers to the param store and to a
    `CS::PlayerGameData`;
  - the walk from the store finds the live tables in about 0.4 s, once;
  - two pointers from the player data lead to `CS::EquipInventoryData`
    objects (seven at once in the lab). Each one holds its record capacity at
    +0x08 and a pointer to its records at +0x10. A record is 0x18 bytes:
    handle, item id, count, list index and two more words. Weapons have
    handles starting with 8 (`0x8080…`).
  - Reading them takes about 12 ms.
- **Weapon instances are not the inventory.** The store of item instances
  (`CS::CSGaitemImp`, an array indexed by the handle's low bits) held 54
  weapons for a new level 9 Vagabond, three of them at +10. The inventory
  held seven: three unarmed slots, Moonveil, the Halberd, the Heater Shield
  and the Twinblade. The extras look like leftovers kept in the save, likely
  from character creation, so only the inventory is read.
- **What counts as raised.** 25 map pick-ups give a weapon at +1 to +10, so
  holding one proves nothing. A held weapon counts only above the highest
  level any pick-up, shop row or class gives it at.
  - Custom-weapon lots (category 6, a weapon with a set level, e.g. custom
    5010 is Rogier's Rapier +8) were not counted as given when the lab test
    above was made. Since 2026-11-02 they are, so that test now needs
    `APOC_NATIVE_LEVELS` for its levels.
- **What is upgraded:** item-lot slots that give a weapon (category 2), and
  rows of ShopLineupParam / ShopLineupParam_Recipe that sell a weapon for
  runes alone. Nova chose to include shops (spec §2.7). Rows paid for in
  items are left alone.
- **In `apoc-rando`:** `gen --auto-upgrade` copies `apoc_native.dll` into
  the mod folder. It takes the DLL from beside the program, or from
  `--native <dll>`. It also writes a me3 profile `<out>.me3` beside the
  folder that loads both, and records the option in the spoiler log.
  - The profile was checked against me3 0.13.0's own source (2026-10-30),
    not by a launch:
    - `packages` and `natives` are the field names (`package` and `native`
      are aliases), and `path` is all either needs;
    - a package without an `id` is named after its path;
    - relative paths are joined to the folder the `.me3` file is in, which
      is the mod folder's parent.
    The CLI turns a profile into the attach config the lab writes by hand,
    and that config loads both in the lab. The release's runtime is
    byte-identical to the one the lab uses.
- **Known limits:**
  - The levels come from what the player holds now. Selling or dropping
    every raised weapon lowers them again; nothing is kept between sessions
    other than the inventory itself.
  - Offline only one inventory is filled. In co-op, another player's
    inventory may be read too (not tested).
- **A pick-up from the ground works** (2026-10-30). The chapel body's
  treasure part was moved, in a test map only, onto the carpet by the start
  (its real place, on a raised step behind a chair and a table, never gave the
  lab the prompt). Holding Rogier's Rapier +8, "Pillage remains" gave
  Twinblade+8 where seed 22 puts a plain Twinblade, and the module's log showed
  10000008 arrive.
- **Lab tools:** `keys.sh` holds a key with `KEY:N` (seconds). `run.sh`
  passes `ER_LEVELS=regular,somber`, a floor for testing.
  - `look.sh DX` turns the camera by moving the mouse DX pixels on the
    game's display (about 45° per 400). The I/J/K/L camera keys do nothing
    there.
  - `e` is the action key; it also closes item dialogs, where Return does
    not.
  - If a run is stopped by hand, `systemctl --user reset-failed
    apoc-er-lab.scope` clears the unit before the next run.

## Phase 5: enemy randomizer

`crates/enemies`, from `docs/spec/enemies.md`. **Nothing here has run in the
game:** Nova asked for no lab tests of it, so it stays off unless `--enemies` is
given. The tests read back what is written, and that is all they can show.

- **What moves.** A part keeps its entity id, so scripts still find whoever stands
  there, and takes another's NpcParam row, AI, equipment and model.
- **Strength, from the game's own tiers.** `NpcParam.scale_a` (+0x58) is a
  SpEffectParam row, and its +0x10 a health rate. Margit's 2521 x 1.656 is the
  known 4,174, and Morgott's same 2521 x 4.125 the known 10,399, held by a test.
  Scale off, a clone keeps the body's own. Which of an effect's other rates are
  damage is not settled, as the tier is copied whole.
- **Clones.** A moved body gets a row from 800,000,000 up, keeping the spot's drops
  and runes (+0x2C, probable), and with scaling its health and both tiers.
- **Models.** A map's model entry: name offset, type 2, its own index, the source
  path's offset, and at +0x18 how many parts use it, always updated. Part names
  still say the old model; 20 of the game's own parts already differ.
- **Size** is from `height` (+0x14) and `radius` (+0x18), both probable: the larger
  of height and twice the radius, under 2.7 small, under 6 medium, else large.
- **Malenia** is one body. Event 15002811 starts her second phase; it is copied to
  the new boss's script and no longer starts at home (`malenia.rs`).

On 2.7.2, seed 1, all moving, 2026-10-30:
- 233 boss spots with 163 different names, and 21,710 ordinary spots (19,968
  small, 1,438 medium, 304 large), 126 of them humans; 21,940 of 21,943 hold someone new;
- 468 maps, 2 scripts and 18,696 new rows, 28 MB, in 80 s with the item shuffle;
- left in place: Beast Clergyman, God-Devouring Serpent, Promised Consort Radahn,
  Messmer the Impaler, Stonedigger Troll, Magma Wyrm Makar, Nox Swordstress, Erdtree
  Avatar and three Scadutree Avatars, as no script read waits on their deaths.

`apoc-fs enemies`, `npc-diff`, `sp-diff`, `entity-events` and `msb-models` show the
data this came from. `crates/enemies/tests/real_enemies.rs` holds it.

Still to do:
- **First seen in the lab** (2026-10-31, seed 1, bosses only, items unmoved, warped
  in with `test-warp --from`): the world and Stormveil loaded at about 7 GB. In
  Margit's arena his own cutscene played, as a cutscene has its own copy of him, and
  then a Godskin Apostle stood there with its name on the bar, and fought: Margit's
  wake, which it may lack, froze nothing. Killing it was not tried.
- **Ordinary enemies crash the game** (same day, seed 1). Moving them, the game
  boots and the chapel loads, then it closes some 30 s after loading around
  Stormveil, the same way each run: a null read on the main thread at
  `eldenring.exe+0x475D2B` (r14 = 0x90000051). A limit of 20, 8 and 1 all fail, and at
  limit 1 with bosses left alone it failed twice; bosses only, from the same save, ran
  100 s. So it is not the number of models. Rows or a particular body remain, untested.
- **What the crash is, from disassembly** (2026-10-31, no lab run; the cause is not settled):
  - `+0x475CE0` updates a character's `CSChrRideModule`. At `+0x475D2B` it calls through the character's
    manipulator (its controller), which is null. r14 is almost certainly a task group id, not about the character.
  - The update runs only with the module's +0x31 set, which its constructor sets when RideParam names the
    character's model as rider or mount; a second writer at `+0x4754C7` looks like network sync and was not traced.
    So the character is almost certainly a rider or a mount.
  - `+0x511103` reaches it through the characters skipped this frame for their distance.
  - `stand_on` changes only the model, NpcParam, think, talk and chara fields, so fields left over from the old
    body (squad, patrol, collision) are ruled out. `enemy-m1` has 2,307 moves involving ride-listed models,
    separating riders from mounts, such as a horse on a foot soldier's spot in m10_00.
  - **Not known:** why the manipulator is null, as the code read has no such path offline once its selector has
    run, or why stock riders are safe. An earlier bosses-only seed also moved ride-listed bodies, with no known crash.
  - **Next, one lab run:** either have `apoc_native` log the character's model, ride state and three manipulator
    slots on that fault, which tells the routes apart and names the body; or filter RideParam's models, c0000
    among them, from `body_ok` and see if the crash stops, a possible stopgap that says less.
- **Mount events, the likely cause, fixed; not yet seen in the game** (2026-11-01, no lab run). Map event type 21 ties
  a rider's part to a mount's (`rider_part`, `mount_part`). `apoc-fs mounts` lists them: 136 tie a part to another or
  to none, 26 are unset, and all 134 full pairs are models a RideParam row pairs. Stormveil's map has 16. Moves changed the
  bodies on those parts and left the event, so the game set up a ride between bodies that have none. Parts in a mount
  event no longer move, as ordinary enemies or as bosses, which keeps Night's Cavalry and their horses in place (three boss
  names leave the boss pool, so seeds that move bosses change). 21,572 spots remain. `real_enemies.rs` holds it. This fits
  the crash but does not explain why the manipulator is null. The add-on keeps ordinary enemies off for now.
- **The crash no longer comes, seen in the lab** (2026-11-01, 2.7.2, one run of each, `ER_CPU=300`, a copy of the same save at
  Margit's fog). Seed 1 with ordinary enemies moved and riders tied in place (default limit of 20 models) ran over 5 minutes,
  3 of them standing and about a minute after walking 6 s away, at 5.8 GB, until stopped. The control, `enemy-m1`, built before the
  fix (limit 1), closed about 35 s after Continue, with its loading screen still up 25 s in, sooner after loading than the 30 s first noted. Its fault address was not captured, and the two seeds differ in
  limit and in item placement as well. Only Stormveil's gate and the nearby open world were loaded, no moved enemy was seen or
  fought, and the rest of the world was not visited, so other crashes there are not ruled out.
- **The rider fix is what stopped it, by a matched control** (2026-11-02, 2.7.2, `ER_CPU=300`, one run of each, a copy of the same save at
  Margit's fog). Seed 1, `bosses=100,ordinary=100,mix=10`, both built today, with only the mount lines of `tied_parts` taken out of one for the control.
  Without them the game closed 41 s after Continue, a null read at `eldenring.exe+0x475D2B`, the same address as the first crash. With them it ran 259 s until stopped, with no fault, the last shot standing at the fog.
  The same build in Limgrave, warped in, also ran until stopped: Bridge of Sacrifice for 266 s, the last shot in the open world and the earlier ones of the same size from about the third on (a horse's tile, `m60_44_34`), and Third Church of Marika for 266 s,
  though there no shot shows the church: loading screens, then the dark hall by about 100 s. The Warmaster's Shack was not rerun. Nothing was fought, and why the manipulator is null is still not known.
  This seed overrides `m10_01_00_00`, and the Third Church run's log lists it while the bridge's does not, so the hall is probably that map, the Chapel of Anticipation by the spoiler log's naming; the earlier reading of the Roundtable Hold looks wrong. Why the character goes there is settled in the next item.
- **Fights, first seen** (2026-11-02, 2.7.2, seed 1 as above, every moved enemy at 1 health by `test-weak`, seven launches, `ER_CPU=300`). None closed; all were stopped by hand. Six Proton logs hold no page fault, and the Gateside launch's was not checked.
  - **A moved boss fell to the player's own hit**, the first such kill seen: the Soldier of Godrick on Margit's spot, under its own name on the bar. "GREAT ENEMY FELLED" showed, 12,000 runes came, and Beast Blood arrived, the spoiler's item for flag 9100.
  - **Enemies at Gatefront attacked and killed the character** on four landings, within seconds in the runs where it was watched: a huge long-limbed creature and a smaller figure, which lock-on caught. YOU DIED ran, and the game reloaded. They were taken to be moved enemies from what stands there unmodded, not from a read of the map. **No ordinary enemy was killed**, so an ordinary enemy's death and drop are not seen.
  - **The dark hall is where the character returns after dying**: seen directly, YOU DIED at Gatefront, then a loading screen, then the hall. So Limgrave's earlier "hall, no open world" runs were deaths at the landing spot, not a fault. Its far door reads "Opens only to those carrying a finger".
  - `test-warp` from a map to a place in the same map does not move the character. Two launches went off a cliff by steering at 10 fps, which is the tool, not the seed.
  - Open for balance: a low-level character dies within seconds at a grace with such a body beside it. The next item measures how often.
- **Ordinary enemies die, and large bodies near graces do not rise** (2026-11-02, same seed, nine more launches, none closing, no page fault in any log read).
  - **A player's hit killed an ordinary enemy** at Gatefront: +77 runes, the body fell, the game ran on. `apoc-fs test-calm` made that possible: every enemy in a mod folder's maps with think 0 stands still, and the character walked up unhurt.
  - **Ten kills by script** (`test-kill`) of different bodies at Stormveil, c6100, c3500, c4491, c6010, c4430, c4385, c4070, c2271, c3670 and c4140, all at 1 health, with no fault. 67 runes arrived; which of the ten paid them, or whether all died, was not seen.
  - **A moved enemy's drop was not picked up.** One hit was tried beside a dark body at Lower Capital Church and the runes stayed 0, so it was not the target or did not die. That hazard is still open.
  - `apoc-fs grace-danger` counts large bodies and bosses within 30 m of a grace, heights not compared: 39 of 419 graces unmodded, 35.7 a seed over 200 seeds at `bosses=100,ordinary=100,mix=10`, at most 41, and 0.5 a seed with a boss sent out by the mix. So seeds do not add this, and no rule was added. The bodies listed beside Gatefront on seed 1 are both classed Small (c4316, c6100), while what killed the character looked huge: it may stand further off, or its NpcParam height may not show its size. Not settled. Sizes read sound for the models checked: dragons Large, soldiers Small.
  - `test-warp` to Manor Lower Level and Stormhill Shack ended in the dark hall with every enemy calm at the Manor, so a landing can itself kill, probably by a fall. A warp onto an enemy's entity landed elsewhere, so it does not work as a way to reach one.
- **Auto-upgrade in the lab.** `upgrade: 1 EQUIP_PARAM_CUSTOM_WEAPON_ST table(s)`: found, so the preset weapon fix has its table. `upgrade: no character's name read`, with a character loaded: **+0x9C is not the name on 2.7.2**, so levels last only until the game closes. Searching 44,000 blocks from the player data for the save's name `Boot` found it twice, one place appearing during loading and the other's address changing between scans, neither in a `PlayerGameData`. The search was dropped, and the fallback stands.
- **The add-on offers ordinary enemies and the mix**, marked untested. Seed 1 at `bosses=100,ordinary=100,mix=10` through `tools/addon-run.mjs`: the same 502 files and spoiler log as `apoc-rando`, by `diff -r` and `cmp`, in 104 s with 2.05 GiB of module memory.
- **Machine checks again** (2026-10-31, 16:36). The PC went down some 25 s after a lab run was
  stopped, its scope having just timed out and been killed, nothing else running. The same
  fatal error: MC5, syndrome 0x4d000000, on another address (Ryzen 7 5700X). The last three
  boots all report it, so this reads as the CPU faulting under load, not as anything a mod or
  a seed asks of it. Stock settings and a BIOS update are what to try. Keep launches few.
- **Machine checks.** The PC went down during the all-enemies run with no kernel
  message, and the next boot reports a fatal CPU error: MC5 (execution unit),
  syndrome 0x4d000000, at a Wine address. Boots of 2026-10-29 and 30 report the same
  on other threads. Not memory: the lab peaked at 6.7 GB of its 9. Two Cyberpunk labs
  ran beside it. Six later launches, alone and at `ER_CPU=200`, did not bring the PC
  down, which rules nothing out. The lab takes `ER_CPU` to lower its CPU quota.
- [ ] **The gate:** the spec's hazards in the game, once a lab run is wanted.
- [ ] **Malenia's trigger.** Read from the scripts, not yet seen in play. 4[14] with
      5 and 0 waits on a body's fall, and 286 uses across bosses run into its death;
      with 5 and 1 only nine exist, on bodies that get up again. No NpcParam field
      sets the nine apart, so what puts a body in that state is not read: it may
      come from something her arena does. Only play will tell.
- [ ] **Other bosses' own phases.** One-body bosses with scripted phases (Godrick,
      say) lose them when moved, as those events stay in the old arena. Only
      Malenia's moves, by Nova's choice.
- [x] **Hostile or friendly.** NpcParam `team`, byte 0x133: teams 0, 1, 2, 26 and 28
      stay. Gideon and the friendly Patches left the boss pool, as they fight only by
      script. Named invaders on 27 still swap.
- [x] **A limit on models per map** (`models=N`, default 20). Seed 1 all moving: 468
      maps gain 7,761 models, at most 24 in one, against 101 with no limit. What the
      game can take is still for the lab.
- [x] **Fixed bosses read.** The Stonedigger Trolls and Makar now move: their scripts
      end the fight on a fall. Bodies that get up as a next stage (Godfrey's first
      form; the summoned ones in map m14) no longer move at all, as they could have left
      a fight unwinnable. Left: Beast Clergyman, God-Devouring Serpent, Promised
      Consort Radahn, Messmer, Nox Swordstress, one Erdtree Avatar and the Scadutree
      Avatars, whose fights end some other way.

## Phase 6: output and merging

`apoc-rando gen` and `check` take `--with-mod <regulation.bin or folder>`, repeated,
lowest priority first. The merge is `fs_param::merge`, lifted from `apoc-fsparams`
without its loose Dark Souls III containers.

- **Order.** The mods' tables are merged onto the game's own, and their maps, scripts,
  talk and text are read over the archives' (`gamedata::GameFiles`), and the seed is
  randomized on the result. So the proof, the scaling and the spoiler log read what
  ships, and any file the seed rewrites is built on the mod's copy: a mod's text stays
  beside the keepsake names. The same seed on different mods gives a different game.
- **Merging tables** compares rows word by word. Mods that change different values
  combine; where two set one value, the later wins and the spoiler log names both.
- **Whole files** cannot combine. Where two mods hold one file, the later is used and
  the run names each.
- **Names are the game's own.** The area map knows items and bosses by name, so a
  mod's renaming changes nothing: text is read from the archives, and a boss keeps the
  name it has unmodded (VINS gives Makar's bar a name of its own). The spoiler log and
  moved bosses' health bars therefore use unmodded names, and a mod's new items, having
  none, stay out of what NPCs wear.
- **Loading.** With mods, `<out>.me3` loads them in order and the seed last, since me3
  lets a later package override an earlier one (`mapping.rs` in its source). It carries
  the DLLs of each mod's own profile, or its whole `dll` folder if it has none. A DLL a
  profile names and does not ship is left out and said.
- **Refusals.** A mod that cannot be merged stops the run with its reason. So does one
  that takes away what the area map needs: emptying the Rold Medallion's lot gives "no
  item Rold Medallion anywhere in the model". A missing game file is named by path.

What real mods showed, each fixed:
- VINS's 8 talk scripts have no name at all (offset -1, length 0), which the ESD reader
  refused. `Esd::nameless` keeps this.
- VINS names `map/MapStudio` in its own case, so mod paths are matched in any case.
- VINS has NpcParam rows at 800,000,000 and up, where enemy moves began their own. New
  rows now follow the highest there.
- Nightreign Movement ships a DLL and no profile.

**Seen in the lab, 2.7.2, seed 1, 2026-10-31** (`ER_BELOW` and `ER_NATIVES` in `run.sh`
load other mods under the seed; `ER_CPU=300`, two Cyberpunk labs beside for the first runs):
- **ELDEN VINS 1.9.1 alone** (made for 1.17.0: 109,115 rows changed, 24,339 added): its
  title screen, and me3 served the seed's `regulation.bin` and `menu_dlc02` with VINS's
  character files beneath. The world loaded at 6.3 GB; through Margit's fog, the fight
  began under his name. Items alone move in this seed, so he stood there.
- **Nightreign Movement and VINS together, all bosses moved:** no crash in any run, up
  to 5.9 to 6.3 GB. The seed's maps and text served, and an Ancestor Spirit stood in
  Margit's arena under its own name, with VINS's title menu text still showing from the
  seed's menu file.
- **A moved boss dies, and the reward is the seed's.** `test-kill` kills the spot's entity
  by the game's own 2004[4], on VINS's copy of the map's script (the character died before
  landing a hit by hand, so it was not killed by the player). "GREAT ENEMY FELLED" showed,
  and Sanguine Amaryllis arrived, the spoiler's item for flag 9100, Margit's Talisman Pouch
  unmodded. The inventory log on a fresh save read (11, goods 2020012, 5): the spoiler's x5.
- **A kill by the player was not landed.** With the boss's AI off (`test-kill --freeze`) the
  character walked up to it unhurt, and swung, but steering by screenshot at 10 fps did not
  connect. Its attacks, the boss's damage and YOU DIED all ran. The death itself is the
  game's, so the same events run; the hit is what is not seen.
- **All five DLLs are in the game's module list** (`/proc/<pid>/maps`): Nightreign's, VINS's
  three and ours, side by side.
- **The other load order, Nightreign last:** its title art, compass and character files are
  the ones served, so the later mod wins as me3's source says. On that seed the character
  creator gave the Warrior the seed's gear, and listed all nine keepsakes as the spoiler has
  them, in order. In the world it walked, attacked and rolled for over a minute, at 6.1 GB.
- **The two mods clash with each other, not with the seed.** Both replace the player's
  character files, `c0000.hks` and `common.emevd`, 18 files in all, and the later is used
  whole. The run lists them. Which wins is the player's load order; those files cannot be
  merged. Compared byte by byte, all 18 differ, the moves among them (`c0000.hks`, 1.05 MB
  against 0.89; `c0000.anibnd`, 7.4 MB against 20.8). So these two mods cannot both have their
  moves: the later's are the character's, and the earlier's DLL runs beside files not its own.
  Whether Nightreign's moves feel right was not judged: a 10 fps lab cannot tell.
- **The profile:** `me3 launch -p`, 0.13, read `<out>.me3` and wrote the attach config the lab
  builds by hand: the mods in order, the seed after them, and `savefile = "prof-seed1.sl2"`. It has
  not started the game, for a reason of ours: run from this repo, me3 takes `target/x86_64-pc-windows-gnu/release`
  under its working folder as where its launcher is, finds none, and hangs with no message, its child
  left defunct. Reproduced with a stand-in for Proton, on the plain desktop too, so it is not the
  headless box, as first written here. Harmless commands ran in the same container under the scope and
  gamescope. Run from elsewhere, or with `me3 --windows-binaries-dir <runtime> launch`, the right launcher
  is found. A player is unlikely to meet this, as the app launches from the game's folder. A launch
  with the right launcher has not been tried.
- `test-warp` from a map to itself reloads for ever: a limit of that tool, not of mods.

**Proof, four threads:** Nightreign Movement alone, 10,000 seeds in 1,023 s; both
together, 10,000 seeds in 1,804 s; none failing on either. With no mods, seed 7's `regulation.bin` and spoiler log are byte
for byte those of the build before merging. `crates/rando/tests/real_merge.rs` holds the
merge, the refusals, the mod's files and rows, and the profile.

`crates/rando` is now a library with `apoc-rando` a thin command line over it: `generate` and `check`
return what they did, and messages go to a callback, for the mod manager to call.

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
- [x] **Shops.** Placed where their merchants stand (below). Of the two
      flags, the second puts a row on sale (below); the first is probably
      the item's own got flag, as the Husks' resale events test it.

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

#### What buying asks

Each shop row in the model carries its terms (`gamedata::model::Terms`), and
the walk counts a row as got only once they are met:

- **Currency.** ShopLineupParam byte 0x18 says what the price is paid in. The
  area map names the item: 1 Dragon Heart (dragon communion), 2 Starlight
  Shards (Seluvis's puppets), 4 Lost Ashes of War, 5 Heart of Bayle.
- **Material cost.** The row's EquipMtrlSetParam set: a remembrance for its
  trades (the Axe of Godrick costs the Remembrance of the Grafted).
- **The on-sale flag** (ShopLineupParam +0x10), explained in one of three
  ways, in order:
  1. **A boss.** An event run that marks one boss defeated (`boss_defeated`
     names it, or else it sets the boss's defeat flag) and turns the flag on.
     121 rows: Enia's boss armour (Radahn's set on 9130, which Caelid's map
     sets just after his defeat flag), the remembrance rebuys, and dragon
     communion's dragon incantations.
  2. **An item handed over in talk** (`fs_esd::handover`, above). 26 rows:
     Sellen's and Corhyn's spells, Seluvis's puppets for his potion, Gowry's
     Pest Threads for the Valkyrie's Prosthesis.
  3. **An `on_sale` entry** in the area map. 27 bell bearings for the Twin
     Maiden Husks' stock, written from the order of the bearings' goods ids,
     which runs with the flags' (8951–8965 with 11109751–765), and each
     bearing's name matching its stock. Also the Husks' resale of the
     Crafting Kit and three other tools once held, and Miriel's eleven
     books.
- **Nothing explains it:** the row is never counted. On 2026-10-29 that is
  58 rows behind 26 flags. `apoc-fs logic` lists them; among them are Patches'
  stock, Thiollier's poisons, Seluvis's copies of the scroll spells, and the
  Husks' Spirit Calling Bell.

Counts are not modelled: holding one Dragon Heart opens every communion row.

Adding the terms showed a fault in the walk: a location giving two items
stopped at the first new one. Messmer's lot gives his Remembrance and then
Messmer's Kindling, so with Enia's free Remembrance now waiting for Messmer,
the Kindling was lost and Enir-Ilim never opened. The walk now takes every
item.

#### What a gift asks

Each gift now carries its ways (`Location::given_if`), read from what holds
on the way to the award in its talk scripts. Any one way is enough, and each
asks for everything it lists:
- **A boss beaten.** Melina gives the Rold Medallion once Morgott is beaten.
- **An item held.** Rya gives the Volcano Manor Invitation for her necklace.
- **Another location got**, by the flag picking it up sets.
- **A talk flag.** A flag set in talk takes on what that talk asks in turn.

Flags that must be off are taken to be off. A way that waits on a flag nothing
explains is never open. A `place` entry in the area map speaks for its gift
instead.

On 2026-10-29, 86 gifts have conditions, and every way of every one of them
waits on an NPC's quest phase. Those are flags such as Varré's 3185–3199,
which shared events move between (common event 3199 for Varré) with
instructions not yet catalogued:
- `1003[01]` skips N lines unless a flag is on;
- `2003[69]` sets a flag (it also sets Godrick's defeat flag);
- `2003[63]` clears a range.

#### Quest stages, read from the events

The events that move quest stages are now read (2026-10-30), so a stage flag
is explained like a talk flag: by what holds on the way to the instruction
that sets it.

- **`fs_emevd::catalogue::runs`** lists every run of an event a script makes:
  the events nothing starts, and each event a run starts, with its call's
  arguments copied in.
- **`fs_emevd::flow`** follows every path through a run and collects, at
  each `set_flag`, the ways to reach it. It works from the control-flow
  entries now in the catalogue (skips, gotos, ends, waits and condition
  groups; see the EMEVD section).
- **What a way may ask:** flags on, explained again in turn (talk, events,
  bosses, handovers, pick-ups); flags off, taken to be off, as in talk; and
  the map, when the event is in a map's own script, which only runs while the
  map is loaded. The logic reads that as reaching the map's area.
- **Conditions not read** (a position, a character's state, a counter, an
  equipped piece) shut the way. Some are surely harmless, but a counter could
  be a count of Great Runes; taking them as met could make a seed impossible.
  Reading them one at a time opens more gifts.
- **What it could not follow** (an uncatalogued control instruction, a jump
  back, a missing label) ends the path. It only ever loses ways.

Of the 86 gifts that waited on quest stages, 29 are now counted: 21 ask for
nothing beyond reaching the NPC, and 8 for a boss, an item or an area. 57
still wait on a stage nothing explains, nearly all because an unread
condition stands on every way to it. `apoc-fs logic` lists each blocking
flag.

The gate was run again with this in place: `apoc-rando check` placed and
proved seeds 0 to 9,999 in 998 s on four threads, none failing (526 s before;
the walk has more gifts to visit, and two lab games of another project shared
the machine).

**The four gifts not missable, read one at a time (2026-10-31).** Three of the
four now open, and only one of them can take a key item. 54 gifts stay shut
instead of 57, and 90 are missable instead of 87.

- **Great-Jar's Arsenal opens, and can take a key item.** It waited on three
  deaths (`if_character_dead`, 4[0] asking for state 1). The Knights of the Great
  Jar are summoned once the Jar is spoken to, and the reward's flag waits for all
  three. It now asks for the map the fight is on.
- **Ruins Map (2nd) and (3rd) open, and cannot.** Count Ymir's maps waited on two
  bells, each an object action (5[2]) done holding the Hole-Laden Necklace (3[4]).
  These read the bells only, and the necklace is now a key item. But the talk script
  makes these gifts either-or: with both bells rung it gives lot 106620 if lot 106610
  was taken, and lot 106630 if not, while lots 106610 and 106630 share one pick-up
  flag.
- **A fault this showed, now fixed.** Lot 106610 (Ruins Map (2nd), Beloved Stardust)
  was counted free and could take a key item. A player who rang both bells before
  talking to Ymir again would never be offered it, losing the key. All three lots are
  now `missable`. Gifts sharing a pick-up flag are marked so by rule. The only other
  such pair (Cross Map, 106100 and 106120) was already missable.
- **Neutralizing Boluses stays shut.** Its flag 10009335 is tested once in its talk
  script (t304001000), and no event, talk script or param row sets it.

So key-capable slots change by none: one gained, one found unsafe and taken away.

`missable` still does not see what a talk script's branches do when a flag's test
fails, so a gift that loses out on an unread branch and shares no flag with its
twin would not be marked. No such case is known beyond lot 106620, which is listed
by hand.

The gate was run again on 2.7.2 with this in place: `apoc-rando check` placed and
proved seeds 0 to 9,999 in 1,379 s on four threads, none failing, with other work
sharing the machine (load 10 to 16).

**What is left shut, and why it stays so.** 53 gifts not ruled out stay shut: the
Boluses and 52 missable ones, which could not take a key item even if read, and
some of which also wait on flags nothing sets. They stay unread: an NPC's death,
for one, may only be possible at one stage of a quest, which the way would not
show. The commonest unread conditions
are 4[0] outside the Jar, 3[23], 1003[3] and 3[12]. Range state 1 (3[1],
1003[3]) was studied and is still not settled: 14 of the 19 ranges 3[1] tests
with it are stage ranges with one flag on at a time, so it is not "all on", but
the code around its uses fits more than one other reading. It stays unread
rather than guessed, as a wrong guess could count a gift the game withholds.

**Missed gifts.** A quest stage can move on before its gift is taken. 90
gifts are marked `missable`: most wait on a flag some event turns off again (by
`set_flag` or by clearing a range), and the rest are the either-or gifts above. They are shuffled like any other slot,
but never take a key item, since a key there could be lost for good. Flags
talk scripts turn off are not counted: talk turns its own flags on and off as
a conversation goes. Of the slots that could take a key item before, only two
are lost: a Jar from Alexander and Gurranq's Ashes of War. Melina's Spectral
Steed Whistle and Rold Medallion stay open.

`crates/logic/tests/real_graph.rs` holds these, and that more than 60
gifts are missable, at least 25 of them got in the vanilla walk, and that
Melina's two gifts are not. `real_items.rs` checks that no key item moves
onto a missable gift:

- every shop row has an area or is ruled out;
- the Crafting Kit is first placed where Kalé stands, in Limgrave;
- a location offered in two places is got at either;
- rows wait for their terms: Radahn's armour for Radahn, the Axe of Godrick
  for its remembrance, Dragonclaw for a Dragon Heart, Sellen's Cometshard for
  the Conspectus Scroll, and the Husks' Smithing Stone [1] for its bell
  bearing;
- gifts wait for their ways: the Rold Medallion for Morgott, the Volcano
  Manor Invitation for Rya's Necklace.

After this, 560 locations are unused (559 shop rows and the Leyndell
medallion half). Only 24 are left "anywhere": 10 classes, 5 script awards
and 9 boss rewards.

#### Review of the marked claims (2026-10-31)

All 35 marked claims are now settled, none by play. `apoc-fs script-find` and `script-dump` read what
the scripts do with an item or flag; the rest was looked up in public guides and compared across
them (Fextralife, Gamepressure, PowerPyx, GGRecon, Game8, Beebom and others), with no randomizer's
material read.

Wrong in the draft, and corrected:
- **Enir-Ilim** opens from the ruins of Rauh, past Romina, with the Kindling, not from Belurat.
- **Rauh is two places.** The ruins and the Church of the Bud are reached through the Shadow Keep, and
  Rauh Base from Scadu Altus. No source joins them, so they are two areas. Viaduct Minor Tower goes with
  the ruins, and Ravine North with the base.
- **The Abyssal Woods** open from the Shadow Keep, through the Darklight Catacombs. Forsaken Graveyard and
  Church Ruins belong to them, not to the Jagged Peak; m40_02 and Castle Watering Hole go with the Keep.
- **The Jagged Peak** waits for the Ancient Dragon-Man, whose Dragon's Pit now counts as the Gravesite Plain.
- **Scaduview** waits for Commander Gaius. The Hinterland asks only for the O Mother gesture, found at
  Bonny Village, which opens before the Keep.
- **Charo's Hidden Grave and the Stone Coffin Fissure** are areas of their own. Guides differ on the way
  in, so each asks the most any of them says. The Fissure's seal is stood for by the Golden Hippopotamus,
  as what lifts it happens at the Keep.
- **The Moonlight Altar** waits for Astel (m12_01 event 12012506 and his arena's two fog gates).
- **Wyndham Catacombs** is on the Altus Plateau.

Confirmed as drafted: the Kindling and the Carian Inverted Statue are needed (scripts); the Spellmachinist's
Bell Bearing gives Rabbath's Greatbolt; Craftsman's Shack is on Mt. Gelmir; the Cerulean Coast is down the
Ellac River; and the other graces and edges of the first pass.

**Still resting on inference, not a source:** the three lists of shop rows are unused because no script
opens them. The way into Charo's and the Fissure may be shorter than asked, which only narrows placement.
Whether the Hippopotamus must fall to go deeper into the Keep, and how the Main Gate is reached, no source
read says, so the Keep's edge asks nothing of them.

Asking too little could make a seed unfinishable; asking too much only narrows where keys may land, so doubtful
ways take the stricter reading. Jori alone is not asked for: his map tile counts as the woods', and a boss
behind his own gate would shut them. Rerun on 2.7.2, three threads, beside two Cyberpunk labs: 10,000 seeds in 1401 s, none failing; the area map and item tests pass.

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
- [x] **Shop requirements** (below).
- [x] **Gift conditions** read from talk (below); NPC quest phases are open.
- [ ] **The play-through review** of the coverage report.
- [ ] **The 293 flagged, named lots** are taken to be unused (one checked in
      play, above). Naming the 99 maps would account for 20 of them.

## The mod manager add-on

The add-on is built as Bodyworks and Motion are: one `.apocaddon` file holding a page
(`ui/`) and a WebAssembly module (`crates/addon`), which runs in a frame inside the mod
manager and reaches only what the mod manager hands it. One file serves every system.

- **The library has no folders in its core.** `rando::build` takes the game's files
  and a `Sink`. `apoc-rando` still writes a folder, and seed 1 is byte for byte what it
  was before the change, spoiler log included.
- **The module asks, the page fetches**, with Motion's `until` loop: first the tables,
  exe and archive headers, then the pieces of the archives a seed reads
  (`Data0.bdt#offset+length`). Between seeds it keeps what it read.
- **Proven outside the window.** `tools/addon-run.mjs` runs the module in Node against
  the stock game, single-threaded: seed 1's 16 files and spoiler log are the same bytes
  as `apoc-rando`'s, in 105 s with 1.29 GiB of module memory. A native single-threaded
  run takes 61 s and 1.14 GiB, or 66 s and 1.97 GiB with enemies moved.
- **The page follows Motion's layout and CSS**, the design system's §6.4ae, with
  Bodyworks' sliders. Checked by eye against a screenshot of Motion's page.
- **The packed add-on, driven in Chrome** with its Build button, against a stand-in
  host serving the stock game: seed 1's 16 files came to `output.write` as the same bytes
  as `apoc-rando`'s, in 2 min 23 s. The file is 882 KB.
- **One new request of the mod manager:** `files.part` with `own: true`, a piece of the
  game's own loose file and never a mod's. It is on the branch `feat/randomizer-addon`
  of `Apocrypha-Mods/apocrypha`.

Left out of this first version, on purpose:

- **Ordinary enemies** were not offered at first, as moving them crashed the game. They
  are now, marked untested, with the bosses and the mix (2026-11-02).
- **Auto-upgrade** needs the native module loaded, which an add-on cannot place yet.
- **Building on other mods.** The module reads the game's own copy only.

Not yet done: the add-on has not run inside the real mod manager window, and no seed from
it has been played. It must not be published before the legal opinion on `oozextract`
(PLAN.md §6), which it needs to read the game's files. Publishing also needs the offline
add-on publisher key.

## Against the specs (2026-11-01)

The code was read against `docs/spec/`. The README now separates our own design
(`items.md`, `enemies.md`, `bias.md`), which the code follows, from the earlier
dirty-room write-up (`00` to `10`), merged from GitHub and kept for reference.

**Fixed:**
- A boss's model no longer counts against the models-per-map limit (`enemies.md` §2).
  Only seeds that move ordinary enemies can change.
- Neither copy of a boss placed twice moves now (§3.3). No boss is placed twice on
  2.7.2, so no seed changes today.

**A fault found and fixed:** Abductor Virgins and Walking Mausoleums should stay
(`enemies.md`), but the rule went by name and no NpcParam row carries either name, so it
caught nothing. It now goes by model. The Walking Mausoleum is `c4450`: all of its
open-world remembrance talk scripts stand on that model (`apoc-fs name-models`), a direct
reading. The Abductor Virgin is `c4470`, from elimination: the models in both the
academy and the Manor, against the guides' list of places and its tall body. No script
names it (the value 9140 the warp events test is no help: `c4470`'s animations do not
hold it, and some 70 other models' do), so that one wants a look in the game. A wrong guess costs little: it keeps one
kind of enemy still.

**For Nova to decide (our own design):**
- A category at 0% still moves in shops and class gear: with `weapons=0, merchants=100`
  sold weapons move. Should a row move only if both sliders pass (`items.md` §2.1)?
- The add-on page offers only the category sliders, DLC and bosses: no bias,
  merchants, starting gear, keepsakes, gathering, NPC gear or scaling (`items.md` §1).
- Slots that never give their item may receive ordinary items (`items.md` §4), and a
  one-off item can land on a slot with a low drop chance.
- An unnamed ordinary enemy on a boss's bar shows blank or stray text.
- The keepsake menu's text is rewritten even at `keepsakes=0`.
- `--bias` for merchants, starting gear, keepsakes and NPC gear is accepted and does
  nothing.

**Where the earlier write-up differs.** None of this was acted on, as taking behaviour
from it is Nova's decision. Most likely to hurt a player first:
1. Doors and talk that check a location's flag and not the item: flags do not follow
   moved items, and the proof cannot see such a door. This could softlock a seed that
   passes, and is the one to read first, from the game's own scripts.
2. Riders, mounts and helpers are moved apart from their owners; this may bear on
   the ordinary-enemy crash, though no link is proven.
3. A moved boss's spot is not checked for water, falls or poison, and a body scripted
   as immortal until a trigger is only partly filtered.
4. Key items on one-time enemy drops while enemies move.
5. Places that close later: the walk treats a reached area as reached for good, so an
   item in the Royal Capital can be lost after the burning.
6. A required key on an expensive shop row, and one-off items on unlimited rows.
7. Open-world far copies of an enemy; a human-model load limit; invaders respawning.
8. An old seed's files staying in a reused folder (`generate` removes nothing; the
   add-on removes them), and a seed sharing the player's save unless a profile is written.

Could not be settled from the code: whether slots that never give their item exist;
that results match across systems (the suite passes on Linux and under Wine);
`crates/native` against the auto-upgrade rules, which was only partly read.

## Failed branches, and what stays shut (2026-11-01)

**Failed conditions are now read.** A talk state tries its conditions in order, so the
one that passes says those before it failed. `fs_esd::reach::must_strictly` counts that,
and `apoc-fs gift-audit` lists what can shut each gift that may take a key item. A gift
that needs a flag off on every way, where an event can turn that flag on, is now
`missable`, so no key item goes there. Six slots changed:
Rold Medallion's (needs the Erdtree unburnt, flag 110, and a key placed elsewhere can let
a player burn it first), Grace Mimic's, the Perfume Bottle's, the Hole-Laden Necklace's,
and both Scorpion Stew gifts'. Flag 9001 is left out: events hold it on only while they run.
Flags that talk turns on are not counted, as talk marks its own gifts given that way: in the
audit's list, each such flag is set under the same conditions as its gift's award. That is a
reading of the conditions, not of the game running.

**Neutralizing Boluses is never given.** Flag 10009335 appears once, as a test in
`t304001000`. No event, talk script or param row holds it (`apoc-fs gift-blockers`), so it
is now `unused` and takes no item. The exe holds the number nowhere either, as bytes or
as text, though code could still compute it. Nothing depends on it either way.

**Five gifts open.** Following flags eight steps back, up from four, opens those whose every
step is read. 49 stay shut, down from 54, all still missable, so none could take a key item.
What held them then: `1003[3]`, `3[1]` and `3[26]`, `3[12]` and `3[10]`, and flags nothing
sets. The next section settles the first three from the exe.

Both changes were proven on 2.7.2 with `apoc-rando check`, seeds 0 to 9,999, none failing: the
failed branches in 1,371 s, then the Boluses and the five gifts on top in 1,122 s. On the mod
manager's `feat/randomizer-addon`, its whole gate passes, with 4,455 Rust tests.

## Event tests read from the exe

Read by disassembling `eldenring.exe` 2.7.2, with no game or lab run. The event instruction
dispatcher is at `+0x569040`; bank 3's table at `+0x570144`. The exe's own class names say what
each test is.

| Instruction | Handler | Class | Meaning |
|---|---|---|---|
| `3[1]`, `1003[3]` | `+0x56F484`, `+0x57047E` | `EcTestMultiEventFlagState`, test at `+0x580210` | byte 1: 0 all on, 1 all off, 2 any on, 3 any off |
| `3[26]` | `+0x56FA5A` | `EcTestIsMyWorld`, test at `+0x57F9B0` | own world when byte 1 is 0, not when it is not |
| `3[10]` | `+0x56F53F` | `EcTestMultiEventFlagCount`, test at `+0x57FEE0` | flags on in a range, against a value: `==`, `!=`, `>`, `<`, `>=`, `<=` |
| `3[12]` | `+0x56F5DC` | `EcTestEventValue`, test at `+0x57D0D0` | a value held in a run of flags, against a value |

This replaces the guess about range state 1, and shows that what the catalogue called
`if_unnamed_state` is `if_my_world`. The first two rows are now in `fs_emevd::flow`. `3[10]` and
`3[12]` are known but not followed: the logic would have to track counts and values, so a way
through them stays shut. That the player's own world is what this test holds in when alone rests
on the class's name; the field it reads (`+0xBCA` of the global at `+0x3D709A8`) was not traced.

**Gifts.** `apoc-fs gift-shut` lists every gift with no open way and what each way stands on: 51
before this, 39 after, so 12 open. That list counts two more than the 49 reported above, whose
count was made differently. Missable gifts stay at 96 of 112.

**A way through a flag that must be off** now marks its gift missable when an event, and no talk,
can turn that flag on, and the rest of the way can happen. Without it, the newly read all-off
ranges would have let key items onto gifts that a quest stage can shut.

**What still holds the 39.** Of the 118 flags they name, 88 have no event setting them, and 82 of
those appear in talk or params. The rest stand on `4[0]`, `3[10]`, `3[12]` and a few others.

Proven with `apoc-rando check` on 2.7.2, seeds 0 to 9,999, none failing, in 1,347 s.

**22 of the 39 rest on flag 4323** (lots 107600 to 107900). It is in no param and is never set by an
event: scripts only test it, and clear it as part of the range 4320 to 4323 or 4324. In talk it is
only the first argument of a call to a shared routine (`t409006100`, `t409106100`), and no game
command receives it. Their other ways come back to it: 2051459743 is set in talk on 4326 and
2051459205, and 4326 is set by event only on 4323. 2051459205 and 2051459740 were found in talk
only. Whatever sets 4323 is outside the files read, so these stay shut; the game running, or more of
the exe, would show it. `apoc-fs gift-blockers` now prints the talk commands holding a flag.

**The other 17.** Flags 3062 and 3063 test the graces of 76200 to 76299 and 76300 to 76399, ranges past
the 64 flags `flow` reads one by one. Tried on 2.7.2: reading them, with a grace's flag meaning its map
reached, opens no gift, as each of those gifts also names another flag. So the change was not kept.
Others stand on `3[10]` (182, ten flags from 170, at least two on) and `3[12]` (3646, 1035449235), which
would need counts and values followed, or on flags with no event setting them (7607 to 7609,
1044362703, 1044362709, 1034509410 among them). These last were read from what `gift-blockers` prints,
not each followed through talk as 4323 was.

## What a moved item leaves behind (2026-11-01)

No game or lab run: read from the scripts and proven by the walk. `apoc-fs flag-readers` lists what reads the
pick-up flag of each key or quest item, and `apoc-fs item-readers` what names the item itself.

**Doors that follow the item.** The Grand Lifts (common event 930, 3[4] with the two Dectus halves, Rold and the two
Haligtree halves), the academy's doors, the Carian Inverted Statue, the Imbued Sword Key and the Unalloyed Gold Needle all
test the item held, so moving them is safe. Reads of `if_item` as "held" are the catalogue's, probable.

**The capital's gate does not.** It tests flags 182 and 105 (m60_45_52 event 1045522500). 182 is on when two of flags
170 to 179 are (common event 730), and those are set by bosses' reward rows and by event 6905 from the rewards' flags; 105
on entering the Roundtable Hold. No script tests a rune held. So a seed with two runes in early chests passed the proof
while the gate stayed shut, and one with none could open it. The edge now asks for two of Godrick, Rennala, Radahn, Rykard,
Mohg and Malenia, and Great Runes are no longer key items. Rennala's flag 177 comes only from event 6905, which starts with
the game's common script, so her count may want a reload; this has not been seen in play.

**Doors on a slot's flag.** The East Altus tower's door and one in the Shunning-Grounds test 400001, the flag of Melina's
gift, which she cannot give after the burning. Those areas take no key or quest item. Which part of the Shunning-Grounds is
behind its door was not read, so the whole map counts. The Divine Towers' restored runes (event 90005110) wait on their boss
and name the rune, which the area map cannot see, so they take none either.

**Places that close.** `closes` in the area map: the capital, the Shunning-Grounds, and the towers reached through them.
Which of these truly shut after the burning was not tested; the reading is the strict one, and it only narrows placement.
Other closings (Volcano Manor after its quest, Farum Azula) are not modelled.

**Where key and quest items go.** Never an unlimited shop row, nor one above 3,000 runes (our own figure), nor an enemy's
drop while enemies move (`Options::enemies_move`). Slots that may not give their item take consumables or upgrades first, and
unlimited rows consumables first, unless the sliders leave none. Prices are still the rows' own (`value`, probable).

**Output.** `<out>.files` beside the folder lists what a seed wrote; the next seed removes what it did not rewrite, and never
anything off the list. A folder with no list is warned about, not touched. `generate` always writes `<out>.me3`, whose save is
the seed's own, though only launching through that profile uses it. The add-on goes through the mod manager, which was not changed.

Proven on 2.7.2 with `apoc-rando check`, three threads beside other work, none failing: 10,000 default seeds in 1,449 s; 2,000
with enemies moving in 303 s; 2,000 with the DLC separate, half of everything moving and keys at 60% bias in 166 s; 500 with
consumables and upgrades held still in 81 s. The `enemies`, `items`, `logic` and `rando` tests pass.

## More links, and more of the world in the lab (2026-11-01)

`apoc-fs mounts` counts what map events tie together. **Generators** (type 5, parts at +0x60, read from the values' shape and
public format descriptions) make copies of their parts and all but 3 of 199 make one model, so their 324 parts no longer move,
271 of them movable before. **Groups** (type 15) stay movable: 457 of 870 already mix models unmodded, and nothing has shown
harm. Summons that scripts spawn were not read.

**Region runs**, seed 1, ordinary enemies moved, one launch each, a fresh copy of the save, `ER_CPU=300`, warped in with
`test-warp`. None closed, and the PC stayed up through all of them, at 5.7 to 5.9 GB.

| Region | Reached | Time from Continue |
| --- | --- | --- |
| Consecrated Snowfield | in the world | 230 s |
| Leyndell | in the world | 230 s |
| Raya Lucaria | in the world | 229 s |
| Scadu Altus | in the world | 230 s |
| Caelid | in the world for about a minute, then a loading screen and a dark hall | about 5 min |
| Limgrave (Warmaster's Shack) | the same hall, no open world seen | about 5 min |

The hall looks like the Roundtable Hold, perhaps Melina's first visit, but this is unconfirmed: the log lists only files the
seed overrides, and it does not list that map. So Limgrave's mounted patrols were not exercised. The screenshots show no moved enemy close by and nothing was fought: this
shows these maps load and run with moved enemies standing in them, not that fights work.

`test-warp` from a map near its target reloaded for ever, seen the same with enemies unmoved, so it now fires once, on flag 10009990.

## Moved bosses: what was read, and what was not filtered (2026-11-01)

No lab run. `apoc-fs boss-commands` lists what scripts tell each spot. Looking for scripted immortality: `2004[10]` state 1 is on
30 spots, but ordinary field bosses use it too, off early in their fight event and on near its end, so it is not that. `2004[35]` and
`2004[73]` sit around cutscenes and stage changes, and their meaning was not settled. No filter was added, as a wrong one pins
bosses for nothing. Water, falls and poison were not filtered either: a death there ends the fight and the reward stays. Which
summons a boss relies on was not read; the defeat events wait on the spot's own bodies, as read before. See `spec/enemies.md` §4.

Nova's decisions: enemies stay on their own side unless `--enemy-dlc mixed`, in the add-on always; a shop row moves only if its
item's category slider allows it as well as the merchants'.

## Far copies, invaders and fights' helpers (2026-11-02)

No game or lab run; the GPU is faulting, so this is read from the files. `apoc-fs far-copies` and `apoc-fs helpers` report it.

**Far copies.** 193 open-world enemy parts sit on coarse tiles, 164 of them movable. No entity is on two open-world parts, and none
has a finer part of its model within 2 m. 28 have one within 30 m, which was not looked at one by one; with no shared entity,
nothing shows they are copies. So nothing is changed.

**Humans and invaders.** Ordinary spots with a human-built body number 58, 32 of them invaders, at most 5 in a map. Trading with everyone reached 7 in
200 seeds at `ordinary=100`. They now trade within their class, and all 200 seeds stay at 5. The game's real limit was not found. This also
keeps invaders on spots whose events keep them gone once dead, which was read from event names and shapes, not from a death in play. It does
mean a human never turns up on a plain enemy's spot: the choice is reversible at `Class` in `crates/enemies`.

**Helpers.** Replaced by the next-but-one section: helpers now go with their boss, or it stays.

**Places that close.** Volcano Manor and Farum Azula stay out of `closes`, from general knowledge and the edges, not a script read.

## Gifts, other systems and the native module (2026-11-02)

No game or lab run.

**Gifts: none opened, 39 stay shut.** `3[10]` was followed as flags (at least some of a range, or none), and `gift-shut` stayed at 39 of 112, 96 missable, so it was
not kept. The `3[12]` tests holding 1035449235 and 3646 read as values held in five flags, at least 3 and at least 4: word 2's low byte
is the flags' number and its next byte the comparison, read from the shape of 249 uses, not the exe. A counter that must climb is a step repeated, which
the area map cannot walk. Shut gifts only take key items away from those slots, so no seed is at risk. Flag 182 stays as read.

**Other systems.** Seed 1, `bosses=100,ordinary=100,mix=10`: `apoc-rando` on Linux and `apoc-rando.exe` (windows-gnu) under Wine wrote the same 501 files and the same
spoiler log, compared by `diff -r` and `cmp`. That is Wine on this machine, one seed. macOS has never seen game files; `a_plan_is_the_same_on_every_system`
now holds one enemy plan on every system CI runs. `generate` also refuses an output folder inside the game's. A DLC check was not added.

**`crates/native` against `items.md` §2.7**, now read through. Two faults, both fixed, neither yet run in the game:
- A weapon given at +7 and then given an Ash of War counted as raised to +7, as levels given were kept per row. They are now per weapon.
- A preset weapon from a lot counted as raised. 16 slots give 10 such rows, so Rogier's Rapier raised every later regular pick-up to +8. They now count as given.
  This needs `EQUIP_PARAM_CUSTOM_WEAPON_ST` found in memory: check `upgrade: 1 EQUIP_PARAM_CUSTOM_WEAPON_ST table(s)` in its log in the next lab run. If it is not found, this fix does nothing and all else carries on.

Checked and sound: ammunition, whose reinforce type has no levels, is never upgraded (353 lot slots); the two paths stay apart; a slot never goes below its own level.
A gap this left, levels falling when a raised weapon was sold, is closed in the next section.

**Proven.** With `APOC_ER_STOCK` on 2.7.2, the workspace's tests apart from `native` (261 passed, none failed) and `native`'s own 7; clippy and fmt clean; `apoc_native.dll` builds. `apoc-rando check`, `bosses=100,ordinary=100,mix=10`,
2,000 seeds in 298 s, none failing. The default 10,000 were not rerun, as nothing here changes item placement or the area map's edges.

## Helpers go with their boss; levels stay; upgraded weapons apart (Nova, 2026-11-02)

No game or lab run: none of this has been seen in the game.

**Companions.** `crates/enemies/src/companions.rs`. A helper whose fight events only put it away (`2004[4]`, `[5]`, `[39]`) goes with its boss; that reading leaves two bosses, Cemetery Shade
and the Grave Warden Duelist. At the new place each takes over the nearest ordinary part with no entity, copied from the boss's part, within 6 m and at its height, keeping its own row at the new spot's strength tier. No table grows. A new event kills them
when the boss dies, and on load once its flag is on, in the shape of event 30052810. At the old place, an event kills the originals on load. A boss with no room is sent home. In a mix, a boss on a spot with no entity brings companions with no such event: they act as ordinary enemies. `crates/rando/tests/real_companions.rs` holds this on the real map.

**Bosses that stay.** 24, from `apoc-fs helpers`: helpers with poses, waits or effects need that arena's script. Each could move only with a script written for it, as Malenia's is. She still moves, leaving her copies, whose part in her fight was not settled. Queen Maggie stays because of one more event reading her adds' effects. A few may stay for a number shared by chance.

**Auto-upgrade.** A slot giving a weapon above +0 is no longer rewritten, and a weapon only given above +0 raises nothing, even smithed further. A weapon given at both +0 and +7 still counts above +7, as an instance's origin cannot be seen.
`Marks` keeps each character's highest levels in `apoc_upgrade_levels.txt` beside the module, by name; a rise counts when seen twice running under one name. So two characters of one name share them, and a deleted character's stay for a new one of that name.
In the next lab run check the log for `upgrade: character <name>`. The name's place, +0x9C in `PlayerGameData`, is from memory of public notes and was not read here. If it is wrong, the log says `no character's name read`, and levels last only until the game closes, carrying over to another character loaded in that time.

**Proven.** With `APOC_ER_STOCK` on 2.7.2, the `enemies`, `rando` and `native` tests (47, none failing), clippy clean, including `native` for Windows. Seed 3, `bosses=100,ordinary=100,mix=10`: the same files and spoiler log on Linux and under Wine. `apoc-rando check`, same sliders, 500 seeds in 72 s, none failing.

