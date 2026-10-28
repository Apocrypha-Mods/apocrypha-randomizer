# 01 — Options

The options model, in our own words. Option *identifiers* below are ours, invented for this spec. Do not reuse the reference program's internal option keys. Defaults are behaviour facts about the reference's out-of-box configuration. Where a default is a tuned number, it is marked *tuned upstream, choose our own*.

## 1. Options storage model

- Options are a flat set of boolean flags plus a few named integer or string values.
- A canonical **options string** serializes the full option state as a sorted list of enabled flags plus `name:value` pairs, the seed(s) and the selected enemy preset name. It is used for export and import ("set options from string"), for the spoiler log header, and it is embedded inside the generated param file as a provenance string (see `08`).
- Options carry a **format version**. Options strings from older versions are migrated or rejected. When the version is bumped, re-running an old options string is not expected to reproduce an old seed.
- Some options are **cosmetic or installation-only** (boss music, ambient music, crash-fix DLL, output mode, merge-mod paths). They are excluded from the "logic string" that feeds the seed derivations, so toggling them does not reshuffle the run (see `09`).
- A short **config hash** (a few digits) is derived from the logic-affecting options with the seed excluded. It is shown to users so two players can confirm they have the same settings.

## 2. Top-level toggles

| Ours | Meaning | Default |
| --- | --- | --- |
| `items.enabled` | Run the item randomizer | on |
| `enemies.enabled` | Run the enemy randomizer | on |
| `seed` | Main seed (unsigned 32-bit; see `09`) | random on first run |
| `enemy_seed` | Optional separate enemy seed. When set and different from `seed`, enemies use it | unset |
| `reroll` / `reroll_enemy` | UI convenience: pick fresh random seed(s) when the run starts | — |

## 3. Item randomizer options

### 3.1 Progression spread ("bias")

- An integer from 0 to 100. Default is low (about one fifth of the range; *tuned upstream, choose our own*).
- At 0, every eligible location is roughly equally likely for every item. As the value rises, it:
  1. widens the weight spread between desirable and undesirable items and locations, so good items drift toward harder or later locations (`03`);
  2. widens a separate spread for key items within an area;
  3. raises the **chain multiplier**. Each time a key item is placed, the area that received it becomes less likely for the next key item, and areas locked behind the placed item become *more* likely. This makes long key-item chains. The multiplier follows a piecewise curve from 1 (no chaining) to about 100 at the top of the range.
- Advanced users can override the four derived weights directly in custom item placement (`3.8`). The values are clamped to sane ranges.

### 3.2 Which locations count as priority locations

The user builds the priority set from groups. The first two are always included.

| Group | Default |
| --- | --- |
| Vanilla locations of major key items | always on |
| Major bosses (all achievement bosses) | always on |
| Other bosses (anything with a boss health bar) | off |
| Golden Seed trees and Sacred Tear churches | on |
| Merchant shops (not spell shops) | on |
| DLC Scadutree Fragment / Revered Spirit Ash pickups (not enemy drops) | on |
| Exclude mini-dungeons from the above | off |

The UI shows a live count of selected priority locations for each group.

### 3.3 Where each premium item class is placed

There are four premium item classes. Each class has a three-way choice: **priority locations only**, **anywhere**, or **not randomized**.

| Class | Default |
| --- | --- |
| Major key items | priority locations only |
| Flask upgrades (Golden Seeds, Sacred Tears) | priority locations only |
| Smithing-stone bell bearings (upgrade-material shop unlocks) | priority locations only |
| DLC blessings (Scadutree Fragments, Revered Spirit Ashes) | priority locations only |

Interaction: if key items are not randomized, logic options that *require* key-item randomization are disabled. Example: delaying Dragonbarrow until Leyndell.

### 3.4 Item placement options

| Ours | Effect | Default |
| --- | --- | --- |
| `spell_shops_keep_type` | Sorcery shops sell only (random) sorceries and incantation shops only incantations | on |
| `smithing_like_vanilla` | Smithing Stone and Somber Smithing Stone availability roughly follows vanilla progression (no +10 materials in the first area) | on |
| `randomize_materials` | Shuffle collectible crafting materials (flowers, fruit and so on) in world pickups. Off means they stay in place | on |
| `add_drop_copies` | For random enemy equipment drops, add guaranteed finite copies to the pool, so weapons and armor normally only farmable can be found | off |
| `dlc_blessings_dlc_only` | Scadutree Fragments and Revered Spirit Ashes are placed only in the DLC (they are useless in the base game) | on |
| `stonesword_master_regional` | Add one "master" Stonesword Key per region that opens every imp-statue seal in that region | off |
| `stonesword_master_universal` | Add one universal master Stonesword Key | off |
| `region_lock` | Each region needs a new special region-lock item to enter. Invisible walls block region borders until the item is held | off |
| `imbued_keys_linked` | Each Imbued Sword Key works only at one specific Four Belfries sending gate | off |
| `serpent_hunter_early` | Give the Serpent-Hunter weapon when Rykard's arena is first entered (vanilla behaviour; turning it off randomizes it) | on |
| `springs_in_pool` | DLC spiritspring unseal stones join the item pool | off |
| `traps` | Add trap items that apply a harmful effect when picked up | off |
| `mark_unchecked` | Add map markers for every unchecked priority location | off |
| `custom_item_mode` | Turn on custom item placement rules (3.8). If off, the custom rules are kept on disk but ignored | off |

### 3.5 Logic options (Great Runes and gates)

| Ours | Effect | Default |
| --- | --- | --- |
| `runes_leyndell` (0–7) | Great Runes needed to enter Leyndell (vanilla gate check) | 2 |
| `runes_mountaintops` (0–7, or unset) | If set, the Rold Medallion is removed and the Grand Lift of Rold needs this many Great Runes instead | unset (keep Rold) |
| `runes_final` (0–7) | Great Runes needed to reach the final boss (activated runes counted at the Erdtree) | 0 |
| `rusty_key_gate` | Gostoc will not open Stormveil's main gate, so the Rusty Key is needed | off |
| `liurnia_via_stormveil` | Reaching Liurnia needs Stormveil (the bypass routes are blocked) | off |
| `late_dragonbarrow` | Dragonbarrow is blocked until Leyndell. Needs key items randomized | off |
| `altus_needs_dectus` | Every route to Altus Plateau needs the Dectus Medallion | off |
| `royal_capital_after_maliketh` | Keep Royal Capital Leyndell reachable after it becomes Ashen Capital, so more Leyndell locations can safely hold required items | off |
| `deathless` | Remove the Volcano Manor abduction route. If off, abduction can be needed in logic. With enemy randomization it still works by dying near the abductor's replacement | off |
| `mark_required_areas` | Kalé sells map markers that reveal which areas hold required items | off |
| `mark_exact_items` | Kalé also sells markers for the exact location of each required item. Needs the previous option | off |
| `early_legacy` | Rusty Key is placed in Limgrave and Academy Glintstone Key in Liurnia | on |
| `early_mohgwyn` | Pureblood Knight's Medal is placed before Leyndell | off |
| `multi_key_per_slot` | Allow several key items in one pickup or drop | off |
| `dlc_keys_separate` | Base-game key items stay in the base game (and DLC key items in the DLC) | on |
| `gaol_keys` | Randomize the DLC Lamenter's Gaol keys. Other major key items may then go there | off |
| `kindling_shards` (count 1–20, required 0–20) | Split Messmer's Kindling into N shards, of which M are needed to burn the sealing tree. Generation can fail if N is large and key locations are few | off (1 shard, i.e. vanilla) |

Interactions:
- Great Rune requirements for Leyndell use the vanilla rune *order* when choosing which runes logic assumes. That stops a late Great Rune from being required to enter Leyndell.
- Replacing Rold with a rune count removes Rold from the item pool. Logic must never reference it.
- Custom item mode, when on, *owns* the DLC key-item placement settings. The DLC-specific toggles are hidden.

### 3.6 Enemy randomizer options

| Ours | Effect | Default |
| --- | --- | --- |
| `scale_enemies` | Scale replacement enemies' health and damage to the scaling tier of the spot they now occupy | on |
| `scale_phases` | Slightly scale multi-phase bosses down, or single-phase ones up, to even out fight length | on |
| `rename_bosses` | Boss health-bar names change to describe the replacement, including mashup names for mixed encounters | on |
| `random_boss_music` | Shuffle boss background music | on |
| `move_boss_runes` | When a boss changes, its rune reward changes to match the new boss | off |
| `swap_multi_boss` | Inside multi-boss mini-dungeon fights, shuffle among the bosses themselves | off |
| `night_bosses_always` | Night-only field bosses spawn at all hours | off |
| `custom_enemy_mode` | Use the enemy preset (3.7) | off |
| `merge_boss_categories` | All boss classes form one pool | off |
| `ignore_arena_size` | Do not use arena-size limits for boss placement | off |

Warnings shown to the user (behaviour): too many bosses or human NPCs placed at once can crash the game or drop the frame rate. Enemies that go silent recover after save-and-quit.

### 3.7 Enemy preset (custom enemy placement)

A preset is a user-editable YAML-like document. Its fields, in our own words:

- **Per-class configuration.** For each enemy class: not randomized; or merged into the parent class (shares the parent's pool); or copy the parent's config; or a custom list of **source groups**. Each group has a percentage weight. The weights must total 100%. A group is a list of enemies or categories, with an optional "pick by type first" flag so each type is equally likely whatever its frequency. A group can also be "not randomized" (keep the original), which lets you randomize only part of a class.
- **Per-class excluded sources** and **globally excluded sources** (enemies that never appear as replacements).
- **Do not randomize** list (enemy instances or categories left in place). It starts with a built-in list of enemies that cannot be moved safely.
- **Frequency adjustments**: "of source type X, keep only K of every T" to reduce over-represented enemies.
- **Manual placements**: specific target instance → specific source enemy (or "not randomized", or "removed").
- **Enemy multiplier ("onslaught")**: a global integer N (and optional per-class overrides) that duplicates every enemy N times. Above about 3x it is unstable and resting at some graces may crash. If onslaught is on, the UI offers to turn off randomization entirely (onslaught alone).
- **"Oops all"**: replace every enemy with one chosen enemy or category.
- **DLC separation**: randomize DLC enemies separately from base-game enemies. Can be set globally or per class.
- **Quick presets**: replace about 10% of regular enemies with bosses; replace wildlife with regular enemies; reduce passive enemies; merge non-major bosses; merge all bosses; split all classes.
- **Item mapping**: limited support for forcing key items and logical milestones in the preset.

The preset has a file name and a description. Presets live in a presets directory, and the UI can import, export and reset them. Some names are reserved.

### 3.8 Custom item placement (item preset)

When `custom_item_mode` is on, a structured rule set replaces the simple choices in 3.3:

- **Placement rules**: for a chosen set of items, where they may go: priority locations, anywhere, not randomized, or custom (a set of location tags or areas). Also "never placed in" exclusions.
- **Priority-location rules**: define extra premium location sets and which items they may contain (for example, dragon-heart shops sell only incantations or legendary items).
- **Item pools**: shuffle a listed set of items only among their own vanilla locations. Variants: weapons by weapon type, armor by armor slot.
- **Not randomized (items)**, **not randomized (locations)**, **removed from the game**.
- **Split item into shards**: any key item can be split into N shards with M required (generalizes 3.5's kindling option).
- **Overall placement scope** and **key-item placement scope**: none, or separate base game and DLC. The key-item scope must be at least as specific as the overall one.
- **Weights**: direct overrides of the four spread and chain weights.

### 3.9 DLC options

| Ours | Effect | Default |
| --- | --- | --- |
| `dlc_sote` | DLC is owned and enabled. Its content is used | on |
| `dlc_tarnished_pack` | Pre-order content is owned. If off, its items are hidden or removed | off |
| `dlc_mix` | Randomize DLC and base game together (vs separately) | separate |
| `dlc_max_weapons` | Every weapon found in the DLC is pre-upgraded to max level | off |
| `woods_horse_fix` | Torrent is not frightened in the Abyssal Woods | off |
| `aging_untouchables_nerf` | Aging Untouchables can be damaged without parrying | off |
| `shuffle_spring_seals` | Each spiritspring stone stack unseals a random spiritspring (seal messages updated) | off |
| `dlc_start` | **DLC Start mode**: skip the base game. Set up at the Chapel of Anticipation, warp to the DLC entry, receive essential items automatically | off |
| `dlc_start_with_base` | Variant of DLC Start that keeps base-game content reachable | off |
| DLC Start extras | Base-game equipment shop at a grace (free); a base-game care package of K items (optionally utility only); Roundtable Hold unlocked at start (otherwise after a major boss or an overworld grace); initial rune level 0–200; all flasks; max flask upgrades; all talisman pouches; all memory stones; all whetblades; all smithing bell bearings | off |

Interactions: with DLC Start plus item randomization, any upgrade the mode grants automatically is removed from the pool. Upgrades it does not grant are placed in the DLC. If DLC content is enabled but the DLC is not installed, the game shows an in-game warning (see `08`), because the run would be uncompletable.

### 3.10 Miscellaneous options

| Ours | Effect | Default |
| --- | --- | --- |
| `random_starting_classes` | Randomize class loadouts. Sub-choice: allow two-handed-only weapons (default), only one-handable weapons, or allow unwieldable weapons and spells; optionally adjust starting stats slightly to fit more weapons; optionally DLC items only | on |
| `random_keepsakes` | Randomize the starting keepsake choices | on |
| `random_npc_outfits` | Randomize NPC armor sets | on |
| `random_ambient_music` | Shuffle area (non-boss) music | on |
| `random_gestures` | Shuffle gesture rewards | on |
| `random_enemy_colours` | Tint enemies with random colours | off |
| `all_maps` | Unlock all map fragments and icons at start | on |
| `all_crafting` | Unlock all crafting recipes at start | off |
| `nerf_malenia_heal` | Turn off Malenia's heal-on-hit | off |
| `nerf_gargoyle_poison` | Turn off the damage tick in the Valiant Gargoyles' poison cloud | off |
| `cheap_upgrades` | Regular (non-somber) upgrades need one stone per level | off |
| `mountaintops_shortcuts` | Add traversal shortcuts in the Mountaintops | off |
| `crash_fix_dll` | Enable the bundled crash-fix helper DLL (memory limits, regulation check, logo skip) | on |
| `helper_dll` | Enable the bundled helper DLL (auto-equip, auto-upgrade and so on). It has its own option set | off |
| `no_stat_requirements` | Remove all weapon and spell stat requirements | off |
| `fog_gate_compat` | Unrestricted placement for later use with a fog-gate randomizer (key-item logic mostly turned off; area gates handled by that tool) | off |
| `fog_dungeon_crawl` | Sub-mode of the above for dungeon-crawl runs | off |
| `nerf_serpent_hunter` | Serpent-Hunter cannot be upgraded | off |
| `alt_placement_profile` | Alternate placement tuned for a community event: fewer late Great Runes, more locations for Altus bell bearings | off |
| Output mode | Mod-loader directory (default) / loose files into an unpacked game / another loader's config | mod-loader dir |

### 3.11 Hidden and developer options

The reference has many developer-only flags (dumps, validations, dry run). We need only: `dry_run` (generate and log, write nothing), `validate` (run the completability checker over many seeds) and `print_logic` (dump the area graph).
