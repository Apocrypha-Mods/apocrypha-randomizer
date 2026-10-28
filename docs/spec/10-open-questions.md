# 10 — Open questions and facts the clean team must derive

## A. Large bodies of facts (derive independently from game files and play-testing)

None of these may be transcribed from the reference. Each needs its own tooling or authoring effort.

1. **Location annotations** (largest item-side task). For every item location in the base game and DLC (on the order of two thousand meaningful entries): logic area, milestone, quest requirements, tags (missable, boss reward, shop kind, mini-dungeon, out-of-the-way, exclusive, item-specific exclusions), and a description we write ourselves. Approach: scrape locations automatically (`02` §2), cluster them by map and position, then hand-annotate with play-testing and community knowledge. Pay special attention to missable classification (NPC questlines).
2. **Area graph**. A few hundred areas and milestones, with map coverage and requirement expressions, covering one-way routes, elevators, Torrent jumps, the capital's ashen transition, abduction, DLC entry, and the gates for each logic option. It must be authored and then verified by the completability simulator over many seeds.
3. **Item groups and desirability tiers**: key items, quest items, premium classes, removable items, the norandom set, priority tiers, per-item lateness restrictions, smithing-stone distribution curve, and bell-bearing region bands.
4. **Enemy annotations** (largest enemy-side task). Every enemy instance (tens of thousands) classified. For every boss, miniboss and evergaol (a few hundred): owner/phase links, flags (defeat, appear, start, encounter, music), groups, arenas (3D boxes), size and environment tags, name data per language, outfit data for human NPCs. Most regular-enemy classification can be automated from NpcParam and MSB data. Bosses need hand work and in-game testing.
5. **Boss event templates**: for each boss, which event scripts belong to it, and how each event argument maps when moved (entity, region, flag, animation, group). This is needed to transplant phase changes, summons, immortality and special scripted attacks. It is very labour-intensive and needs extensive in-game testing.
6. **Item-check templates**: for every script (EMEVD and ESD) that checks an item or its flag (gates, lifts, NPC dialogue branches, the rune gate, duplication, cookbooks, bell bearings), how to rewrite it when the item moves.
7. **Arena-size whitelists** for large bosses and dragon perch points.
8. **Hint geography**: grace-to-area mapping, marker positions per area, and area display names (`PlaceName` text exists in game; groupings are ours).
9. **Starting-class pools and keepsake buckets**.
10. **Event-script instruction definitions** (bank/id → argument types). Build from public community documentation of the format, not from the reference's bundled definition file.
11. **Param layouts (paramdefs)** for every table we touch. Public community sources exist; check their licences. The reference bundles a set of XML paramdefs, which we must not copy.
12. **Vanilla file manifest** (path, archive, size, hash) for the supported game version. Generate it ourselves from a clean install.
13. **Flag ranges** safe for new persistent and temporary flags, and entity-id ranges safe for new entities and regions.
14. **Scaling tier table**: the mapping from map/instance to tier, and the per-tier multipliers. Read from `SpEffectParam` and the MSB/NpcParam effect fields. The dampening and DLC correction factors are our own tuning.
15. **Merge config schema**: our own field names (the concept is in `08` §4).
16. **Translations** for all our UI and in-game messages.

## B. Behaviour unknowns (decide or test)

1. **Archive extraction**: the reference relies on a bundled extraction step and community libraries for the packed-archive, compression and format readers. We need our own Rust (or licence-compatible) readers and writers for: BHD/BDT archives, DCX compression (including the game's newer formats), the regulation encryption, BND4 bundles, PARAM, MSB (Elden Ring variant), EMEVD, ESD, FMG and TAE. Licences of any crates used must be checked.
2. **Regulation size limit**: the exact size ceiling and the compression the current game version accepts.
3. **Crash-fix DLL**: the reference's bundled DLL patches memory limits and the regulation version check. Without it, mixing base-game and DLC enemies tends to crash. Do we build our own (separate clean-room effort), rely on a third-party loader feature, or limit enemy mixing by default?
4. **Human-NPC load limit**: the safe number of simultaneous character-creator NPCs per area. Not documented; needs testing.
5. **Onslaught stability ceiling**: the reference warns beyond 3x. Test our own limit.
6. **Region-lock walls**: exact border coverage, including Torrent jumps, spiritsprings, teleport traps and sending gates. Needs a full world audit.
7. **Abduction route with enemy randomization**: verify the teleport stays attached to the replaced abductor.
8. **Great Rune flags vs items**: confirm, on the current game version, which flags the gates read and which are set on pickup vs on restoration.
9. **Retry policy**: the reference fails the seed; we may retry deterministically. Decide whether a retried seed counts as "the same seed" for sharing. (It must, if derived deterministically.)
10. **Fog-gate compatibility**: whether we support a third-party fog-gate tool at all, or build our own later. That decides whether the fog-compat options exist.
11. **Archipelago-style multiworld**: the reference UI has an integration for an external multiworld system (external items and locations, fake item rows, flag tracking). It is out of scope for this spec unless Nova wants it; it would need its own clean-room spec.
12. **Helper DLL features** (auto-equip, auto-upgrade): out of scope; separate product.
13. **Seamless co-op specifics**: which of our scripts desync, and how to make them co-op-safe.
14. **Game updates**: process and tooling to regenerate the manifest, re-scrape, and diff annotations after each patch. The reference keeps a "stable" copy of its annotations next to the in-development ones. We should version ours too.

## C. Tuned constants to choose ourselves

- Progression-spread curve (weight-spread exponents and chain-multiplier curve).
- Location weight factors (lateness, difficulty, shop aggregation, demotions).
- Price bands and caps; drop-rate formula.
- Scaling dampening per tier step; DLC↔base damage correction; phase health adjustment.
- Starting-class stat fudging caps and weapon-choice falloff.
- Golden Seed share at priority locations.
- Default value of the progression spread.
