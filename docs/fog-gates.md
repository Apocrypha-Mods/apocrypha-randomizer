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

## Open-world borders as doors (Nova's idea, 2026-11-06; research, not built)

Nova: the borders the game sets for online play in the open world could be doors too.
What the files say (`scratch/region-research/`):
- **The regions are a texture, not points.** Each 1 km tile (`m6X_XX_YY_02`) has
  `/other/mapinfotex/…mapinfotexbnd.dcx`: a 256×256 BMP, 4 m a pixel, whose red byte is a
  row of `MapPieceTexParam` (`_m61` in the DLC); that row's word at 0x0C is the
  PlayRegionParam id. World X = XX·1024 − 128 + col·4, Z = YY·1024 − 128 + row·4, rows
  bottom-up (58 of 61 of PlayRegionParam's own safe positions fall in their own region this
  way). The DLC's rows can split by height (an i16 at 0x14, a region below and one above).
  Point type 43 (418 in the open world) overrides it locally; legacy dungeons set it on
  collisions (type data +0x20).
- **PlayRegionParam** (593 rows): 0x04 the multiplayer group (1000 Limgrave, 1200 Weeping,
  2000 Liurnia, 3000 Altus, 3200 Gelmir, 4000 Caelid, 5000 Mountaintops…, 8000 Gravesite
  Plain on in the DLC), 0x14 parent, 0x54/0x58 a safe spot, 0xEC a grace flag.
- **No script can read the region**: no condition tests it. A border door needs trigger
  boxes of ours along the border line, tested with `3[2]`.
- **Borders worth a door** (the roads, not the cliffs): the Bridge of Sacrifice
  (Limgrave–Weeping, m60_44_34), by Gael Tunnel and the Smoldering Church (Limgrave–Caelid,
  m60_46_39/40), the top of the Dectus lift (Liurnia–Altus, m60_38_50), Altus–Gelmir
  (m60_36_52, m60_38_53), Altus–Capital Outskirts (m60_42_52, m60_43_53), Outskirts–
  Mountaintops (m60_46_51), the Ensis Greatbridge (Gravesite–Ensis front, m61_46_43).
- **Plan:** for each chosen crossing, two strips of tall thin boxes either side of the
  line; entering the near strip then the far one warps (`2003[14]`) to a landing 40 m inside
  the paired border's far region (PlayRegionParam's safe spots and graces give the ground
  height); a cooldown flag stops a warp straight back. In the room map each crossing is an
  edge between two open-world areas, rewired by the seed as the other doors are; the
  medallion lifts keep their items.
- **Hazards:** cliffs (Limgrave–Liurnia, Liurnia–Altus are mostly cliff: no doors there),
  Torrent's jumps and spiritsprings arcing over a strip, the DLC's stacked regions, the
  type-43 overrides near lifts.
- **Lab first:** whether `3[2]` catches a rider at speed in a tall rotated box; whether a
  warp mid-ride or mid-air lands well; whether a tile takes many new points.
