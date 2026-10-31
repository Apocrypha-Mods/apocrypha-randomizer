# 05 — Enemy model

## 1. Enemy instances

- An **enemy instance** is one enemy placed in a map (an MSB "enemy" part). Its identity is its **entity id** when it has one. Otherwise it is (map, part name), and the program can assign a fresh entity id if needed.
- Elden Ring overworld tiles exist at several levels of detail. An instance placed on a fine tile can have a **duplicate** on a coarser tile, so it is visible from far away. Both copies must be edited together, or the far copy removed.
- Some maps have no event script. Their enemies borrow a neighbouring map's script file for new events.
- Each instance has static data from the game: model (character id), NPC param row (stats, drops, team, think/AI row), position and rotation, draw groups, collision part, initial animation, any talk (ESD) and character-init (equipment for human NPCs) rows.

## 2. Classes

Each instance gets one class (our names):

| Class | Meaning |
| --- | --- |
| Regular | Ordinary enemy. Need not be killable |
| Wildlife | Passive or non-hostile animal enemies. By default a sub-pool of Regular |
| Scarab | Item-carrying scarabs. Only merge into Regular; never shuffled among themselves |
| Boss | Major boss with a health bar, defeat flag, fog-walled arena and scripted encounter |
| Minor boss | Set up like a boss (health bar, arena, segments) but secondary, such as mini-dungeon bosses |
| Miniboss | Named field enemy without a full boss setup. Should be killable. Sub-pool of Boss or Minor boss |
| Night miniboss | Field bosses that spawn only at night |
| Dragon miniboss | Overworld dragons |
| Evergaol | Bosses inside evergaols (special arenas) |
| Hostile NPC | Human NPC invaders and hostile human NPCs. Never merged with other classes by default |
| Caravan | Caravan trolls (special movement and grouping) |
| Helper | Not placed alone; always travels with an owner enemy (summons, adds, mounts, phase-2 bodies) |
| Spectator | Pseudo-class for "adds" around bosses |
| Not randomized | Excluded (scripted, cutscene-bound, unstable, quest-critical) |

Classes form a **hierarchy** used when the user merges pools. Regular is the root; boss classes nest under Boss; the miniboss variants nest under Miniboss, with Minor boss as an alternative parent. Each class has flags: may be split by base game vs DLC; may never be randomized among itself; may never be merged.

**Categories** (for presets and UI) are named groupings: all enemies, all bosses, every class, every class split by base and DLC, every enemy *type* (a model family, such as "Tree Sentinel"), individual named instances, and keyword groups (all dragons and so on). Categories show counts in the UI. The category list must be generated from our own enemy data.

## 3. Per-instance metadata (schema)

The annotation layer for enemies stores, per instance:

| Field | Purpose |
| --- | --- |
| class | As above |
| owner / next phase | For helpers: the owner. For two-phase bosses where phase 2 is a separate entity: the next phase |
| add | An enemy likely aggroed together with this one |
| defeat entity, defeat flag, appear flag, start flag, encounter flag, music flag | The scripting hooks for bosses and minibosses |
| phase count, health-bar index | For multi-phase and multi-boss bars |
| start and end events | The event scripts that drive the encounter |
| groups | Script entity-groups the enemy belongs to; extra group to leave when moved; buddy (spirit-summon) group |
| arena | One or more 3D boxes (position, size, rotation) covering the fight area. Excluded sub-boxes (invalid teleport spots). Extra named arenas for special placements. An optional game sound region that covers the arena |
| enemy type | Used to stop the same type filling several spots in one encounter |
| name data | Display name, partial name for mashups, proper-name flag, adjectives, per-language name keys |
| tags | Below |
| item name | The item-randomizer name of the boss (to link drops and rewards) |
| split-from / dupe-from | Derived entities, such as a boss variant created as a regular enemy |
| region overrides | Regions hard-coded in AI scripts that must be moved too |
| talk lines | Lines that must be removed or delayed (death lines delayed until the defeat flag is set) |
| outfit | For human NPCs: armor set |
| scaling tier | The vanilla scaling tier of the spot |

Tag vocabulary (our names):
- **unkillable-exclude**: cannot be killed in this spot, so not allowed where killing is required (fall damage zones, water, etc.);
- **perceptive / sensitive**: aggro ranges or senses that break in some spots;
- **multi**: part of a group fight;
- **size: small / medium / large** and **arena: small / medium / large**: placement size limits;
- **noplace**: may be a source but never a target;
- **no-swamp**, **no-fallout**, **no-strand** and similar: environmental exclusions (poison swamp, fall-death planes, stranding spots);
- **invader**: NPC invader setup;
- **generated**: spawned by a generator, not placed directly;
- **allow-frames**: allowed in maps with scripted frame or level-of-detail limits;
- **main-helper / team-helper / combat-helper / command-helper**: helper roles;
- **immortal**: immortal by default (scripted), must be handled;
- **resist**: has special status resistance;
- **night**: night-only;
- **drop**: carries a drop that the item randomizer cares about;
- **no-souls**: gives no runes;
- **singleton / unique-arena**: special constraints;
- **name modifiers**: proper name, adjective, partial name and so on.

The reference's enemy annotation file is very large: tens of thousands of instances, a few hundred bosses and minibosses. Deriving our own is the single largest data task (see `10`).

## 4. Arenas and positions

- Every boss, miniboss and evergaol target needs an **arena** so the program can (a) place the replacement at a sensible point; (b) teleport helpers and adds; (c) put multi-boss replacements at distinct points; (d) keep teleporting or flying bosses in bounds.
- Arena points can be chosen **independently of the seed**, so re-randomizing does not change positions unpredictably. The reference uses its own non-seeded randomness here. We should make it deterministic too (`09`).
- Specific bosses need **special arenas**. Example: large flying dragons need a perch point and support for "tree" landing spots, or else they are boring or stuck.

## 5. Scaling tiers

- The game applies a per-spot stat-multiplier effect to each enemy (in `SpEffectParam`), selected by area. Base game and DLC each have a series of tiers: about 20 for the base game and a separate DLC series. Human NPCs use a different, inconsistent series.
- The program builds a map from instance to tier. It reads the effect each instance already has. Instances without one inherit the most common tier in their map.
- Some instances are flagged "no scale up" or "scale per phase".
