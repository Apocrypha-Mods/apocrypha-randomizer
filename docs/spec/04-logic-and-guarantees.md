# 04 — Logic and completability guarantees

## 1. What "completable" means

A run is completable if, starting from the beginning area with the starting inventory, the player can reach the final boss and every area that the ending requires, collecting items only from **non-missable** locations, using only routes valid under the chosen options. If DLC is enabled, the DLC's final area must also be reachable. The DLC entry needs its vanilla prerequisites: two specific base-game bosses defeated and the DLC entry interaction reachable.

The vanilla game needs two Great Runes to enter Leyndell and the Rold Medallion to reach the Mountaintops. The program generalizes this (`01` §3.5).

## 2. Model reduction

Before placement, the graph is preprocessed:
1. **Expand** dynamic expressions from options (rune counts, shards, region locks, gates).
2. **Resolve area-to-area requirements recursively** until every area's requirement is an expression in **items only**. Cycles are broken by dropping self-referential branches. The graph has real cycles; for example, the capital connects to the sewers, which connect to the deeproot region, which connects back.
3. **Collapse equivalent areas**: areas that are mutually reachable with identical item requirements are merged for weighting, so tiny areas do not distort probabilities. Merging uses a disjoint-set style grouping, and an area can name a "weight base" area it should be counted with.
4. Compute a rough **difficulty** for every area: the minimum number of key items needed to reach it.

## 3. Key-item placement algorithm (behaviour)

- Key items are placed **one at a time, in random order**, with some items forced to the end. Endgame keys go last, which spreads interesting placements and lowers failure rates. Shards of one item are treated as a group.
- For the item being placed, compute the set of areas that **do not unconditionally need that item**, given everything placed so far. The item may go only in one of those areas. This is the core invariant that stops circular locks.
- Choose among allowed areas by weight:
  - an area's base weight is roughly proportional to its number of eligible key-capable locations;
  - **chaining**: after an item lands in area A, A's weight is divided by the chain multiplier, and every area that depends on the item has its weight multiplied by it. Higher progression spread gives longer chains;
  - a mild lateness bonus;
  - per-item constraints: `early_legacy` restricts Rusty Key to Limgrave and the Academy key to Liurnia; `early_mohgwyn` keeps the Pureblood Knight's Medal before Leyndell; base and DLC scope; and options that restrict certain keys to certain regions.
- After each placement, update requirements so the next item sees the new state. Example: once item X is in area A, everything behind A now also needs whatever A needs.
- **Quest items** are then assigned to areas. Their constraint: they must not be in a location that transitively depends on themselves. They may be missable if the item is not required, but they prefer non-missable. Quest-gated locations then get an "effective area" for weighting.
- **Spirit-ash-like unlockers** (an item whose possession unconditionally unlocks other unique items) are placed before other quest items, with extra location capacity.
- When the area for every key item is fixed, the general placer (`03`) chooses the concrete location inside that area. Area capacity was checked during assignment: an area cannot receive more key items than it has eligible locations.
- **Failure and retry.** In a small fraction of seeds the heuristic paints itself into a corner. The reference then fails with an error that tells the user to try another seed. A better implementation could retry with a derived sub-seed, but it **must stay deterministic** (see `09`).

## 4. Great Rune handling

- The runes logic assumes for the Leyndell gate are chosen by taking the Great Runes in **vanilla acquisition order**. This avoids late runes such as Malenia's being required for the capital. With rune count N, logic requires "N of the first several runes". Any N runes work in-game, but logic proves completability with the early ones.
- When the Mountaintops gate becomes "N runes", Rold is removed and the gate check is rewritten (see `07`). The Rold-related milestone used by "until" logic is cleared when it is no longer a single item.
- A **free rune** concept exists for logic only: at high counts, one rune may be treated as given, to keep generation feasible.
- Great Runes must be **activated** at a Divine Tower to count for the final-boss requirement; the Leyndell gate checks possession. The final-boss requirement (`runes_final`) is enforced by a new script gate at the final approach.
- The vanilla gate counts runes by **flags**. The game sets each rune's flag when the rune is obtained. Moving runes needs the flag to follow the item, not the location (see `07` §3).

## 5. Missable content (why it is excluded)

Locations get the **missable** tag, and are never used for required items, when:
- an NPC questline can fail, or the NPC can die or be killed, leave or turn hostile (many shop and gift locations);
- the location exists only before a world-state change: the capital burning (Leyndell becomes Ashen Capital); Farum Azula and Crumbling areas after progress; Volcano Manor after the questline;
- the location is one of several mutually exclusive choices;
- the location is behind something enemy randomization can break, for example a regular enemy drop whose enemy might be replaced by something unkillable or not present;
- the location's carrier is a timed or one-time event (a dialogue with a branch, an invader that despawns);
- getting the location needs an online or multiplayer mechanic;
- the location is in content not owned (DLC or pre-order).

Locations available only **until** a milestone are allowed to hold required items only if logic proves the item is reachable before the milestone. Key items use dedicated area-level logic for this.

## 6. Hazards the logic must model

- **Abduction routes**: the Volcano Manor abductor can teleport the player into a later area. If `deathless` is off, logic may treat being taken as a route. With enemy randomization the route still works if the player dies near the abductor's replacement. The program must keep that teleport event tied to the replacement.
- **One-way drops and elevators**: some areas are entered one way only. The graph is directed and must model that.
- **Leyndell burning** removes access to Royal Capital locations unless `royal_capital_after_maliketh` is on, in which case scripts are changed so the capital stays reachable after Maliketh (see `07`). Logic changes to match.
- **Dragonbarrow early access** to Altus (via the Dectus lift exception) is a known logic trap. The reference keeps one logical special case so Dectus is still placeable when Dragonbarrow is late, and rejects option combinations that would truly softlock.
- **Region lock**: every region's areas get the region item as an extra requirement, and the starting region needs none. Torrent jump points and teleporters that skip region borders must be accounted for, either blocked or modelled.
- **Fog-gate compatibility**: most area logic is dropped; only item-level restrictions and scope remain.
- **Shards**: all shards are placed behind normal gates, whatever count is required, so logic stays simple.
- **Option validation**: incompatible combinations are rejected before generation with a clear message, for example "late Dragonbarrow without key randomization". Further hazards: too many shards for the available locations; key items set to not randomized while a logic option needs them moved; DLC keys scoped to the DLC while DLC is off.

## 7. Completability verification

After placement, the program must **re-simulate**: start from the beginning area with the starting inventory, then repeatedly collect every item from every reachable non-missable location and expand reachable areas until nothing changes. The final milestone must be reached. If it is not, generation fails with an internal error. This independent check is cheap and must run on every seed. The reference relies largely on construction-time guarantees; we require the explicit check.

The same simulation produces:
- the set of key items actually **required** for the ending (for hints);
- a rough area **order** for the spoiler log;
- per-area **lateness** used in weighting.
