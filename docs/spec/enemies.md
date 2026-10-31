# Enemy randomizer: behaviour spec

Written 2026-10-30 from Nova's own design decisions, in place of the dirty-room
write-up the spec README first planned. No upstream material was read to write
it, so it needs no contamination review. Hazards are found from the game's own
scripts and from play.

## 1. The principle

As for items (`items.md` §1): the player decides how random a seed is, and a
seed and its options always give the same game.

## 2. Options

`--enemies bosses=N,ordinary=N,mix=N`, each 0–100%. Nothing moves unless given.

- **Bosses** trade places with bosses. At 100% all of them move.
- **Ordinary enemies** trade places, each one individually, not type by type.
- **Mix:** the share of bosses that trade places with an ordinary enemy. The
  boss goes to an open world, and the ordinary enemy takes the arena.
- **Size.** Ordinary enemies trade only within their size, small, medium or
  large, so no dragon lands in a tunnel. Arenas are not limited by size.
- **Scaling**, on unless `--no-enemy-scaling`. A body takes the health and
  strength tier of whoever stood there, so Malenia in Margit's arena is about
  as tough as Margit. Off, it keeps its own.
- **Base game and DLC.** `--dlc separate` keeps enemies on their own side too.
- **Models per map:** `models=N`, 20 unless given, 0 for no limit. The most models
  ordinary enemies' moves may add to one map; a boss's model does not count. Swaps
  bring maps down to it, so every body still lands once, but where no swap fits a
  map stays over: on seed 1 a limit of 8 reached 13, and 20 reached 23.

## 3. Rules

### 3.1 Who may stand where

- **No boss stands in more bodies than the unmodded game has.** Each body lands
  once. A boss that exists once appears once; Night's Cavalry, placed eight
  times, can appear up to eight times (Nova's choice).
- **A boss with two bodies is two bosses.** Radagon's spot and the Elden Beast's
  each take a boss of their own, and after the cutscene a new boss shows up. The
  final fight moves like any other.
- **Rewards stay with the place.** Whoever stands in Margit's arena gives
  Margit's reward, and an enemy's spot keeps its drops and runes, so the item
  plan and its proof hold.
- **Friendly people stay:** anyone with a talk script who is not a boss, anyone
  on a team that does not fight the player unprompted (NpcParam `team` 0, 1, 2, 26
  or 28: merchants, Millicent as an ally), and named characters not built like a
  player, such as Blaidd or Alexander.
- **Hostile humans swap** (Nova's choice), with their equipment: Millicent or
  Anastasia as invaders (team 27) are hostile, and go.

### 3.2 Malenia

Her two phases stay together wherever she lands, even in a one-phase boss's
arena. They are one body, and an event in her arena's script sets off the second,
so that event goes with her, naming the entity she now stands as.

### 3.3 Left in place

A boss stays put when:
- no script waits on its death, as the fight might then never end. A fall that the
  same event calls a defeat counts as its death (4[14] with 5 and 0: the Stonedigger
  Trolls, Makar). This holds first stages that change some other way, such as the
  Beast Clergyman: Maliketh can still be anyone, and appears when the Clergyman's
  stage ends;
- it gets up as a next stage (4[14] with 5 and 1: Godfrey's first form, and the
  summoned bodies in map m14). It seems never to fall plainly, so in another arena
  that arena's fight might never end. Malenia alone moves, with her script;
- it is friendly until a script turns it (Gideon, the friendly Patches);
- its talk script gives an item;
- it is placed twice, or is a stand-in.

An Abductor Virgin stays, as its grab is a way to the Volcano Manor, and a Walking
Mausoleum, as it carries a shop.

## 4. Hazards

To be filled in as they are found. Not yet seen in the game:
- **Arena scripts act on the newcomer.** They play the old boss's animation ids and
  effects on whoever stands there; what a body does with an id it lacks is unknown.
  Read from the scripts: none of the waits on one body's own effects sits in an
  event that shows a health bar or calls the defeat, and the defeat events wait only
  on a fall, a death or no health, so no reading yet shows a fight that cannot end.
  The likeliest fault left is a body that has the arena's start pose (ids 30000 up)
  but not its wake (20000 up), and stands asleep. `apoc-fs boss-commands` lists what
  each spot is told. Which ids a body has is a rough reading of its `.anibnd`: 106 of
  340 commands were not found on the spot's own body, so some archives hold more.
- **Models per map.** The game's limit is unknown. Its largest map lists 50, the
  median 8; with no limit a seed adds up to 101 to one map, with 20 at most 24.
- **Cutscenes** show or expect the old boss.
- **Quest humans.** A hostile human moved away may be part of a quest's event.
- **The health bar** text is shared where two bosses use one id. The first wins.
- **New rows.** A seed adds up to some 19,000 NpcParam rows, and whether the game
  minds is unknown.

## 5. Required outcomes

- The same seed and options always give the same result.
- An item's place in the proof does not change, as drops and rewards stay.
- Every body lands exactly once.
