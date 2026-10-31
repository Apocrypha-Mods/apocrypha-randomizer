# 08 — Inputs, outputs, merging and failure behaviour

## 1. Inputs

- **Game installation**: the user picks the game executable. From its directory the program needs the packed archives (or an unpacked game) and `regulation.bin`.
- **Vanilla extraction and verification**: on first run (and after a game update) the program extracts the files it needs from the game's archives into a private cache. It verifies each one against a bundled manifest of expected **path, archive, size and hash** for the supported game version. If a file is missing or differs, the program stops with a message naming the file, its expected hash and its actual hash. Users often replace game files with modded ones, so unverified files are never copied into the vanilla cache. The program supports **exactly one game version per release**. Data files are tied to that version.
- **DLC content**: if the DLC option is on but the DLC archive has no content, the program stops before randomizing with a clear message.
- **Bundled data**: our own annotation data (locations, areas, logic, enemies, events, item names, hint areas, sub-maps), an instruction definition table for event scripts, and param layout definitions (paramdefs). All of it must be authored or derived independently. See `10`.
- **Optional merge sources**: one or more other mods (§4).
- **Options and presets**: from the UI, an options file, or the command line.

## 2. Outputs

- A **mod directory** that mirrors the game's file layout: `regulation.bin`; `event/` scripts; `map/mapstudio/` MSBs; `msg/<lang>/` text bundles; `script/talk/` ESDs; and a few `sfx/` and `chr/` files. The set of written files changes from seed to seed.
- **Stale-file cleanup**: on every run, files this program wrote last time but not this time are deleted from the output, or restored from backup in loose-file mode. A file manifest of what the last run wrote is kept for this. Users are told never to copy individual files by hand.
- **Mod-loader config**: a loader config file listing the output directory, any merged mod directories in priority order, and any helper DLLs. The UI can launch the game through it.
- **Spoiler log**: in a `spoiler_logs/` directory. The file name contains the date/time, the seed and the config hash. It holds the full options string, the presets used, item placements and hints, and enemy placements.
- **Options export**: a small file holding the options string, seeds and the enemy preset, so another player can reproduce the run.
- **Zip export**: bundles the output plus merged mods plus a launch script, for sharing (co-op).
- **Save backup**: on the first randomization, a backup copy of the user's save file is made. Never overwrite an existing backup.
- **Compression**: written files use a compression the game accepts. The regulation file must stay under the game's size limit; the reference switched to a stronger compression when a size ceiling was hit. Choose our own.

## 3. Installation modes

1. **Mod-loader directory** (default): output goes to our directory, and a mod loader loads it.
2. **Loose files into an unpacked game**: output overwrites files inside an unpacked game directory. **Before** the first overwrite, each original is backed up next to it (with a backup suffix of our own choosing). A "restore backups" command puts the originals back. Running from inside the game directory is refused, so the tool cannot clutter or overwrite itself.
3. **Third-party loader config**: generate a config for another loader.

## 4. Merging with other mods

- The user can add another mod as a directory, a single `regulation.bin`, or a mod-loader config file (all its listed directories are merged, first match wins, in the loader's resolution order).
- For every file kind the program edits, if a merged mod supplies that file, **the mod's file is the base** instead of vanilla. The program applies its edits on top and writes the result. The output must then take priority over the merged mod in the loader order.
- **Param merging**: rows the mod added or changed are kept. The program's edits overwrite only the fields it needs.
- **Item lots and shops added by mods**: lots and shop rows that are not in our annotation data are **not randomized** by default, because their semantics are unknown.
- **Merge config file** (a small YAML file a mod author can put at the mod's root): lists of entity ids, item lots (map and enemy), lot flags, shop rows and items (`category:id`, or vanilla names) to **exclude** from randomization. If a lot or shop row is excluded, everything sharing its event flag is excluded too. Mod authors are warned never to list vanilla key-item locations, since that would softlock.
- **Version mismatch**: a mod built for an older game version may fail to merge. The program reports which file failed. Some mods simply cannot merge with item randomization, and the program should say so instead of producing broken output.
- **Fog-gate randomizer**: has its own install order. Item and enemy output is generated first with fog-compat options, then the other tool merges our directory. Merging a fog-gate output *into* this program is refused with a message.
- **Language bundles**: text bundles are written for every language, so mods that only change some languages still merge.

## 5. Runtime failsafe (in game)

- The output includes a **consistency check** that runs in game. The generation step computes a fingerprint of the params and texts that change from seed to seed. Map scripts (attached to grace entities, which exist in nearly every map) compare against the expected values. If the loaded regulation does not match the loaded scripts (partial install, files from different runs, or a re-run while the game was open), a flag is set and a pop-up says the files do not match and must be reinstalled.
- A separate check shows a pop-up if DLC content is used but the DLC is not owned.
- These pop-ups use a dedicated text entry. If even that text is missing (very broken install), the game shows its own placeholder text. The help docs explain that symptom.

## 6. Failure behaviour summary

| Situation | Behaviour |
| --- | --- |
| Game dir invalid or exe not found | Refuse; ask for a valid exe |
| Game version unsupported / file hash mismatch | Refuse, naming the file and hashes; suggest verifying the game install or updating the randomizer |
| DLC option on, DLC missing | Refuse before randomizing |
| Program run from inside the game directory | Refuse |
| Merge target missing or wrong kind | Refuse with a message |
| Incompatible options | Refuse before generation with a message naming the conflict |
| Key-item placement fails for this seed | Error "try a different seed"; write nothing |
| Not enough priority locations | Error suggesting more location groups or fewer shards |
| Unplaceable regular enemy | Fallback placement + warning in log; continue |
| Unplaceable boss | Keep original + warning; continue |
| Unknown or modded lots/shops | Leave unrandomized, log them |
| Write failure mid-way | Partial output must not be left looking valid: write to a temp location and swap, or rely on the runtime failsafe |
| Game updated after generation | The game may refuse an old regulation (it checks its version). Tell users to regenerate after updates, or play offline |

## 7. Co-op notes (behaviour)

- Co-op mods need every player on identical files. Supported ways: share the options file (all players generate the same files), or share the zip.
- Some script choices avoid co-op desync. Flags that resync are set once on load, not polled in a loop, and "sticky" flags are used for items that can be consumed.
