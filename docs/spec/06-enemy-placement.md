# 06 — Enemy placement, scaling and encounter rewiring

## 1. Pools

- For each class (after merges from options or preset), build a **target list** (instances in that class to be replaced) and a **source distribution** (what may replace them).
- Default source for a class: the class's own enemies, so a boss becomes another boss and a dragon miniboss another dragon. With base/DLC separation (default on for most classes), base-game targets draw from base-game sources and DLC targets from DLC sources.
- Preset source groups give weighted mixtures (`01` §3.7). "Pick by type" first chooses an enemy type uniformly, then an instance of it. Without it, frequent types dominate.
- Frequency adjustments thin out over-represented sources.
- Excluded sources are removed. If every source of a class is removed, fall back to the class's default sources with a warning, rather than leaving an empty pool.
- **Copies across classes.** If a boss is chosen as a source for a regular spot, the program creates a **regular-enemy variant** of that boss: its own NPC param row with reduced stats, no boss bar, and a rune drop. If a regular enemy is chosen for a boss spot ("oops all" modes), it creates a **boss variant**: health bar, defeat hook, buffed stats. Variants are created on demand and cached.
- **Old-dragon special pass**: a handful of large dragons that cannot be moved freely get a separate pass. Some may be swapped and some stay. The count depends on the distribution.

## 2. Assignment order

- Forced placements first (manual preset placements, and options that pin bosses).
- Then classes from most constrained to least: bosses → minibosses → NPCs → regular.
- Within a class, run a randomized matching. Pull a source from the distribution and test it against the target's constraints. On failure, try further sources. If nothing fits after a bounded effort, use a fallback ("place the original" or "the least-bad candidate") with a log warning. The program **must not** fail the whole run for a single unplaceable regular enemy. For bosses it must use a valid placement or the original.
- Each class gets its own random sub-stream derived from the enemy seed, so changing one class's settings does not reshuffle the others (see `09`).

## 3. Placement constraints (target ← source)

A source may not go to a target if any of these apply:

- **Size**: the source is larger than the arena or space allows, unless `ignore_arena_size` is on. Large flying bosses are tested against a stricter list of arenas checked by hand.
- **Environment**: the source cannot be killed or cannot function there (deep water, fall-death planes, poison swamps where a small enemy dies instantly, spots where it falls through geometry).
- **Kill requirement**: if the target's defeat drives progression (a boss, a key drop, a milestone), the source must be killable in that spot.
- **Repetition**: no same-type duplicates within one multi-boss arena or encounter. Avoid putting the same boss in the same spot as the vanilla boss. Avoid two of the same hostile NPC in one multi-NPC encounter.
- **Interest**: avoid putting interesting enemies (bosses, unique minibosses) in temporary or phantom spots where they vanish.
- **Frames / level-of-detail**: in maps with draw or loading limits, only sources tagged as fitting may go to non-essential targets.
- **Perception**: perceptive or sensitive sources are kept out of spots where their senses break the encounter.
- **Divine-Dragon-like statics**: only allowed in arenas that support them.
- **Phase adjacency**: with multi-phase bosses, the two phases should not end up as the same enemy.
- **Hard progression rules**: options that need specific bosses in specific places (for example, keeping an early mandatory boss beatable). Also rules that stop bosses with notorious roadblock potential from appearing at early mandatory checks.
- **Human-NPC load limit**: only a few human NPC characters (character-creator models with equipment) can be loaded at once. The program enforces a global cap on how many human NPC sources are placed into regular spots.

## 4. Making a replacement work in its new spot

For every replaced instance:

1. **Map edit**: change the part's model, NPC param, think/AI param and character-init to the source's; move it to the arena's primary point (bosses) or keep the target position (regular enemies); copy needed fields (collision, draw groups, initial animation). Underground or ambush idle animations must not be copied if they would put the enemy inside the floor.
2. **Helpers**: bring the source's helpers (adds, mounts, summoned allies, second-phase bodies), with platoons and generators re-created for the new map. Remove the target's helpers that belonged to the old enemy.
3. **Events**: the target's encounter events must now run on the new enemy. Fog walls, boss bar (name, health bar index), music start and stop, defeat flag set when the new enemy dies, rewards awarded on that flag. The source's *own* behaviour events (phase transitions, special attacks scripted by events, summons, immortality removal) must be **transplanted** into the target map, with entity ids, regions and flags remapped. This is the hardest part. It is driven by per-boss event templates that describe which events belong to which boss and how each argument maps (entity, region, flag, animation and so on).
4. **Immortality**: some bosses are scripted to be immortal until a phase trigger. When moved, either keep the trigger working or remove the immortality.
5. **Talk**: transplanting talk scripts breaks most of them. NPC talk is removed except where needed. Death lines are delayed until the defeat flag is set.
6. **Teams**: hostile-vs-friendly team type must suit the slot (an NPC invader turned regular enemy must not respawn forever; allies must stay allies). Invaders placed in regular spots must not respawn after defeat.
7. **Night-only enemies**: either keep the night gating or, with `night_bosses_always`, remove the time check (AI, think param, effect and script).
8. **Status resistances and environment**: enemies moved into poison or rot environments may get extra resistances so they do not die passively. Lava is handled the same way.
9. **Threat level**: the game uses a "threat level" on enemies to choose combat music. After moving, threat levels are re-assigned so a weak enemy does not trigger boss music and a boss in a regular spot does.

## 5. Multi-phase and multi-boss encounters

- A boss whose phase 2 is a separate entity counts as **one** placement unit. The target slot receives both phases of the source, or the source's single phase.
- Encounters with several bosses (duos, trios) are placement groups. With `swap_multi_boss`, mini-dungeon multi-boss fights can shuffle among themselves.
- Health bars are re-indexed so two bosses in one arena show two bars with correct names.
- `scale_phases`: when a two-phase boss replaces a one-phase boss (or the reverse), its health is reduced or increased a little to keep the fight length comparable (*factor tuned upstream, choose our own*).

## 6. Enemy onslaught (multiplier)

- Each instance is cloned N−1 times, with spawn positions offset inside the arena or near the original. The clones get new entity ids and are added to any groups the encounter waits on.
- Encounter scripts that wait for "the boss dies" must wait for **all copies** to die, and the defeat flag is set only then. Music and health-bar handling must cope with several bars.
- Known hazards: crashes above about 3x at some graces; enemies not loading in crowded areas; co-op desync if a player dies (players may need to warp away).

## 7. Scaling

- With `scale_enemies`, every replaced enemy whose source tier differs from the target tier gets a **new scaling effect**. It multiplies the enemy's stats by the ratio between the target tier's multipliers and the source tier's.
- Stats scaled: health, the attack-damage groups (physical and elemental), status buildup, defence, and rune reward.
- The ratio matrix is built from the game's own tier effects. A **dampening** factor shrinks the change per tier step, because late bosses are tuned by hand and the raw ratios over-scale (*shape: geometric dampening per tier; exact constants tuned upstream, choose our own*).
- **DLC ↔ base adjustments**: DLC damage is tuned much higher than base-game damage. Cross-scope moves get an extra corrective factor on damage (health is roughly fine without it). Base → DLC raises damage; DLC → base lowers it. *Tuned upstream, choose our own.*
- The DLC's own player buff system (Scadutree blessing) interacts with DLC enemy damage. The scaling must still be plausible if a base-game enemy ends up in the DLC, where the player has the blessing.
- Human NPCs: their vanilla scaling is inconsistent, so they use a separate simplified rule (the most common tier in the map).
- Without `scale_enemies`, enemies keep their own stats. Only boss-to-regular and regular-to-boss variants are still adjusted.
- The scaling effects are created for every tier pair, even if unused, so the spoiler log can show them.

## 8. Rewards

- Boss **item rewards** (remembrances, key drops) stay with the **spot**. The item randomizer decides what the spot gives. They are awarded on the slot's defeat flag, which the new boss now sets.
- With `move_boss_runes`, the **rune** reward of the spot changes to the source boss's vanilla rune amount (scaled). Otherwise the spot keeps its own rune amount.
- Regular enemies keep the spot's drop behaviour. Item-randomizer enemy-drop locations are tied to the *spot*, not to the enemy model, so randomized drops land where the spoiler log says.
- Variants created for bosses-as-regular-enemies get a modest rune drop.

## 9. Names and music

- With `rename_bosses`, the boss bar name becomes the source's name. For encounters mixing several sources, the program builds **mashup names** from name templates (proper name, partial name, adjective) per language. New name entries are added to the text files.
- With `random_boss_music`, boss music tracks are shuffled among boss encounters (keyed by defeat flag). Music start events and the music-to-character conversion param are edited.
- With `random_ambient_music`, area music tracks are shuffled among area sound regions, skipping regions where much combat happens or which have no unique track.

## 10. Spoiler log

Separate sections for boss, miniboss and regular placements: "target (location) ← source". Bosses are ordered by tier, then id. Regular enemies by map, then id. The scaling table and the preset used are included.
