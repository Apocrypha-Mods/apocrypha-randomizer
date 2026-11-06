# Fog gate randomizer: design

Asked by Nova (2026-11-06): fog walls and dungeon doors lead somewhere else, both at once
(Nova's choice), switchable on and off, separate from the item randomizer but able to run
with it in one seed, the seed then proven finishable with both. Clean room as everywhere
(PLAN.md §1): facts from the game's files, the room map authored here.

## What the game has (research, 2026-11-06; `scratch/fog-research/`)

- **Boss fogs.** 191 fog assets (AEG099_001/002/003, asset parts with entity ids): 123 entrance
  fogs (common event 9005800, or a map's own copy), 56 arena-side fogs (9005811/12/13) that
  only block while the boss lives, 5 seals, 7 one-offs. 9005800 waits on the fog's prompt
  (`3[24]`, ActionButtonParam 10000 "Traverse the mist"), walks the player through
  (`2004[14]`, animation 60060, towards the arena region), and starts the fight when the
  player is in the arena region (`3[2]`). **No warp**: the arena is physically behind the
  wall; its region (`…2800`) sits 1 to 3 m past the fog in 110 of 133 rows; the near side has
  no region. The fog goes when its boss dies.
- **Dungeon mouths.** Caves, catacombs, tunnels and graves are maps of their own (m30–m32,
  m34, m39, m40–m43), joined to their open-world tile seamlessly (ConnectCollision parts, a
  connection point whose position is the offset between the maps). No event, no fog. 62 of 69
  have a "return to entrance" region `AABB2840` (used by the game's warp after the boss); none
  has a point outside.
- **Warps.** `2003[14] [map bytes, region or entity, 0]` moves the player to a point of any
  map (the DLC Start's warp is one).

## The design

**Doors.** Each door has an outside side and an inside side, each a landing point (a region)
and a trigger.
- A boss fog: outside = a new point 2 m before the wall, mirrored from the arena region; inside
  = the arena region. Trigger = the fog's own prompt.
- A dungeon mouth: inside = the dungeon's `AABB2840`; outside = a new point in the open-world
  tile, the inside point moved by the maps' offset and stepped out of the mouth. Trigger = a new
  fog wall, copied from a game fog, at the mouth, so the player chooses to pass.

**The shuffle.** Outside sides are paired with inside sides one for one: through door A from
outside lands at the inside of σ(A); back through σ(A) from inside lands at A's outside. Two-way
always. Not shuffled: seals and locked gates (Leyndell, Morgott's seal, Ranni's rise, Miquella's
seal, the Elden Beast), one-way links (waygates, Mohgwyn, Farum Azula, the Ashen Capital, the
DLC's arm), fogs with no fight start, evergaols, and the start.

**What is written.**
- MSB: the new points (outside landings, triggers) and the mouths' fog assets, by `Msb::insert`.
- EMEVD, per map: an event per shuffled door side that waits on its prompt with the player on
  that side, then warps (`2003[14]`) to the paired side. The 9005800 start of a shuffled boss
  fog keeps running for the fight; the fog itself is kept present after its boss dies so the way
  back stays.

**The room map** (`crates/fog/data/elden_ring.toml`). The item logic's 55 areas stay; rooms
are carved out of them: each dungeon map a room, each boss arena a room (its rewards in it),
legacy dungeons split at their boss fogs by their graces. Each room names its parent area and
its doors. With fog gates off, rooms fold back into their areas and every seed is as before; on,
each door's edge is rewired by the shuffle, and the item proof walks the new graph.

**Proof.** Every seed: the goal reached and every quest item got by the walk over the rewired
graph, as the item checker does now; a fog seed with items unshuffled is proven the same way.

## Lab gates (each must pass in the game)

1. A warp to a new point of an open-world tile and of a dungeon map lands there.
2. A fog asset added to a map shows, prompts, and our event warps on it.
3. A warp into an arena region starts that boss's fight (9005800's `3[2]`), or our event must
   start it.
4. A kept fog after its boss's death still prompts and warps back.
5. A whole seed walked: five doors in a row, there and back.

## Stages

1. `crates/fog`: doors read from the game (boss fogs, mouths), with their sides; tests.
2. The shuffle and its proof over the room map (rooms first for the mouths and arenas).
3. Writing the points, assets and events; a seed builds.
4. Options: `--fog-gates` on the command line, a section of its own on the page.
5. The lab gates above, then the room map refined in play.
