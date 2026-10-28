# Apocrypha Randomizer — plan

An original item and enemy randomizer for Elden Ring, written in Rust, that runs
natively on Linux, Windows and macOS. It is proprietary (all rights reserved) and
lives in a private repo. Players run it as a command-line tool first; later it
becomes an add-on inside the Apocrypha mod manager.

Started 2026-10-29. This file is the handoff: read all of it before writing code.

## 1. Why this is a clean-room project

The best-known randomizer for these games is thefifthmatt's SoulsRandomizers.
Its licence is source-available but **all rights reserved**:

- its code may not be ported, translated or bundled into another work;
- its data files (item and enemy annotations, configs) may not be used either;
- modified versions may be used only privately.

So this project copies nothing from it. It is a fresh randomizer that reaches
the same kind of result by its own route.

### Rules for everyone writing code here (the clean room)

1. **Never read SoulsRandomizers' source, data files or its `dist*` folders**, and
   don't read anyone's port of it. If you have read it in the past, tell Nova
   before you start.
2. **Never copy GPL or unlicensed code.** That includes SoulsFormats /
   SoulsFormatsNEXT (GPL-3.0) and Paramdex (no licence). Reading a format
   *description* is fine. Taking code, or field tables wholesale, is not.
   Permissive code (MIT, Apache-2.0, BSD, zlib, CC0) may be used as a
   dependency; record its licence in `docs/dependencies.md`.
3. **Behaviour comes from `docs/spec/`, not from upstream.** A separate "dirty
   room" read upstream and wrote a behaviour-only spec: what each option does,
   what the in-game hazards are, and what outcome is required. The spec was
   reviewed for contamination (no code, no data tables) before it was committed.
   If the spec is unclear, ask for it to be extended. Don't go and look.
4. **Facts come from the game.** Item locations, enemies, flags and entity ids
   are extracted from the player's own game files by our tools. The area and
   logic graph (which places need which key items) is authored here, from play
   and from general public knowledge, not taken from another randomizer's
   tables.
5. **Game files are never redistributed.** The tool reads them from the user's
   install. Tests that need real files skip when the game is absent; fixtures
   are synthetic.

## 2. Decisions already made (Nova, 2026-10-29)

| Question | Decision |
| --- | --- |
| First game | Elden Ring (base game, then Shadow of the Erdtree) |
| First version | Items **and** enemies |
| Licence / repo | Proprietary, private: `Apocrypha-Mods/apocrypha-randomizer` |
| Delivery | Native CLI first, then an Apocrypha mod-manager add-on |
| Later games | DS3 and Sekiro, once Elden Ring works; keep the core game-agnostic |

## 3. What already exists to build on

Nova's mod manager (`apocrypha-desktop`, private, also Nova's own code, so it
can be reused here) already has:

- **`crates/apoc-fsparams`**: reads, merges and writes `regulation.bin` natively,
  covering AES-256-CBC, DCX (zstd/zlib), BND4 and PARAM rows. It was written
  clean from observed files. It does *not* know field layouts (paramdefs).
- **`crates/apoc-me3`, `apoc-modengine`**: how Elden Ring mods are deployed. me3
  overlays files at runtime, so the game folder is never written to. Our output
  should be a normal mod folder that the mod manager (or a me3 profile) loads.
- **Generator tools** (`generator_cmds.rs`, `apoc-domain/src/tool.rs`): the app
  can already run the *original* randomizers under Wine and import what they
  write as a mod. Our native tool will slot into the same place, with no Wine.
- **`crates/apoc-gamedef/profiles/elden_ring.toml`**: install detection and
  the regulation key.

Copy what you need into this repo, or depend on it by path while it is
private. Don't edit `apocrypha-desktop` from here.

## 4. Architecture

The refined crate layout and build order are in
[`docs/research/formats.md`](docs/research/formats.md) §4; the sketch below is
the shape.

```
crates/
  dcx/        compression wrappers: Kraken (decode), zstd, zlib
  archive/    Data*.bhd/.bdt: decrypt, name hashing, read members
  bnd4/       binder read/write
  param/      PARAM rows + our own field layouts for the tables we touch
  emevd/      event scripts read/write
  msb/        map files (enemies, treasure, assets) read/write
  fmg/        text read/write
  gamedata/   extraction: item locations, enemies, flags → our own model
  logic/      area graph, key-item requirements, completability checker
  items/      item placement
  enemies/    enemy placement, scaling, boss handling
  output/     writes the mod folder, merges with other regulation mods
  cli/        `apoc-rando` binary: options, seed, spoiler log
```

The core stays game-agnostic: Elden Ring specifics sit behind a game trait plus
data files, so DS3 and Sekiro can follow.

## 5. Phases and their gates

Each phase is done only when its gate passes. Record the evidence in
`docs/progress.md`.

**0. Setup.** Install Elden Ring (Steam, Proton), note the exact patch, and keep
a stock copy for testing that the tool reads from instead of the live install.
*Gate:* the game runs unmodded, and runs with an empty me3 profile.

**1. Read the game's files.** Archives (decrypt, hash names, a file-name list),
DCX including Kraken decoding without Windows' Oodle DLL, BND4.
*Gate:* every member of every archive extracts, and the count matches the name
list. BND4 and DCX we write round-trip byte-for-byte.

**2. Formats.** PARAM layouts for the tables the randomizer touches (item lots,
shops, NPCs, characters, spEffects and so on), EMEVD, MSB, FMG.
*Gate:* every vanilla file of each kind round-trips byte-identical (read →
write → same bytes). Fuzz every parser (`cargo fuzz` or a plain fuzz test).

**3. Game model.** Extract every item location (map treasure, enemy drops,
shops, event rewards, NPC gifts), every enemy placement and boss, and the
flags that track them. Author the area graph and key-item requirements.
*Gate:* coverage report (counts by kind and area), reviewed by playing through.

**4. Item randomizer.** Placement under logic, a checker that proves each seed
can be finished, deterministic seeds, and a spoiler log.
*Gate:* 10,000 seeds generated and all proven completable by the checker. One
seed boots and its first area plays correctly in game.

**5. Enemy randomizer.** Classification, swaps, scaling, boss rules, and the
event fixes the spec lists (arenas, phase changes, cutscenes, quest NPCs).
*Gate:* the spec's hazard list tested in game, one item per row.

**6. Output and merging.** A mod folder me3 loads. Merge with other
`regulation.bin` mods (reuse the `apoc-fsparams` merge). Refuse clearly on
unknown patches or missing files.
*Gate:* works alongside two popular regulation mods.

**7. Cross-platform release.** CI builds and tests on Linux, Windows and macOS.
*Gate:* the same seed gives byte-identical output on all three.

**8. Mod-manager add-on.** A tab in the Apocrypha mod manager, output placed as a
mod. The existing add-ons (Bodyworks, Motion) show the pattern.

## 6. Biggest unknowns (attack first)

- **Kraken on Linux:** many Elden Ring files are Oodle Kraken-compressed and
  the game's decoder is a Windows DLL. The only pure-Rust decoder,
  `oozextract`, is labelled MIT but calls itself a port of GPL code, so keep it
  behind a feature flag in its own crate and get a legal opinion before it
  ships. Fallbacks: run the game's own DLL under Wine through a tiny helper, or
  write a clean-room decoder. Writing can likely use zstd or uncompressed files; verify that the
  game accepts them through me3.
- **Archive file names:** the archives store name hashes, not names. The name
  list's licence and origin need checking.
- **Param field layouts:** Paramdex has no licence, so layouts for the tables
  we touch must be learned from data and documented here.
- **The logic graph:** the largest body of hand-authored facts in the project.
  Budget for it. `docs/spec/10-open-questions.md` lists every body of facts we
  must derive (location and enemy annotations, boss event templates, item-check
  rewrites, flag ranges and more).

## 6a. Decisions still open (Nova)

- **Crash-fix DLL.** Mixing base-game and DLC enemies tends to crash without a
  helper that raises the game's memory limits. Build our own (a separate clean
  effort), rely on a loader feature, or keep base and DLC enemies separate by
  default.
- **Fog-gate randomizer compatibility.** Support a third-party fog-gate tool,
  build our own later, or neither.
- **Archipelago multiworld.** In or out of scope.

## 7. Working rules

- Work lands on `dev`; `main` only by pull request.
- Commit subjects describe the outcome in plain words (no `feat:` prefixes).
- Authorship is the human responsible. No AI co-author trailers.
- Never commit game files, extracted data, `scratch/`, keys or `.env`.
- Tests are proportional to risk: round-trip and fuzz tests for every parser,
  property tests for placement and the checker.
- Performance matters: measure a full run (extract → randomize → write) and
  keep it in `docs/progress.md`.

## 8. Docs in this repo

| Path | What |
| --- | --- |
| `PLAN.md` | This plan |
| `docs/spec/` | Reviewed behaviour spec from the dirty room (arrives after review) |
| `docs/research/formats.md` | Clean formats and libraries research (arrives after review) |
| `docs/dependencies.md` | Every dependency and its licence |
| `docs/progress.md` | Phase gates and their evidence |
