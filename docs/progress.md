# Progress

Phase gates from PLAN.md §5 and the evidence that each one passed.

| Phase | State | Evidence |
| --- | --- | --- |
| 0. Setup | done 2026-10-29 | Patch recorded, stock copy made, game boots unmodded and through me3 with an empty attach config (below). |
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
