# Item randomizer: behaviour spec

Written 2026-10-29 from Nova's own design decisions (route C in the spec
plan, item half). No upstream material was read to write it, so it needs no
contamination review. Where it says *proposed*, the wording is ours and waits
for Nova's confirmation.

## 1. The principle

**The player decides how random a seed is, not us.** Every choice below is an
option the player sets. Where the answer is a matter of degree ("how likely is
a key item to land in the DLC?"), the player sets the degree, not just on or
off. A seed and its options together decide the result: the same seed with
the same options always gives the same game.

## 2. Options

### 2.1 What moves

- **Everything can move**: one-off pick-ups (treasure, one-time enemy drops,
  event and boss rewards, NPC gifts), merchants' stock, and starting gear.
- The player chooses, **per category of item**, whether it moves. Categories
  (*proposed*):
  - key items;
  - weapons;
  - armour;
  - talismans;
  - sorceries and incantations;
  - Ashes of War;
  - spirit ashes;
  - upgrade materials (Smithing and Somber stones, Golden Seeds, Sacred
    Tears);
  - consumables and crafting materials;
  - quest items (§2.4).
- A category that does not move keeps every one of its items where the
  unmodded game has them.

### 2.2 Merchants

- Shop stock can be shuffled with everything else, as its own choice.
- *Proposed:* an item placed in a shop is priced by its kind, and a shop's
  limited stock stays limited.
- Whether key items may be sold in shops falls under §2.3: a shop is one more
  kind of place, with its own weight.

### 2.3 Where things land: the player's bias

- Key items may go **anywhere the logic can reach in time**. Nothing is ruled
  out by us.
- The player can weight **kinds of place** up or down, from "never" through
  "even" to "strongly preferred". This applies to key items, and *proposed*
  to every category. Kinds of place (*proposed*):
  - open-world pick-ups;
  - dungeons (catacombs, caves, tunnels, castles);
  - boss rewards;
  - NPC gifts and quest rewards;
  - shops;
  - the DLC (Shadow of the Erdtree).
- "Never" removes a kind of place for that category; any other setting only
  changes how likely it is.

### 2.4 Quest items

- Quest items (e.g. Rya's Necklace, Seluvis's Potion, Irina's Letter) can be
  shuffled or kept in place: the player's choice, with the same bias as §2.3.
- When shuffled, the logic must keep each quest finishable: a quest's item is
  placed where it can be reached before the quest asks for it.

### 2.5 Base game and DLC

The player chooses one of:
- **mixed:** one shuffle across both; base-game items can land in the DLC and
  DLC items in the base game, within the logic;
- **separate:** base-game items shuffle among base-game places, DLC items
  among DLC places.

The DLC weight of §2.3 applies to the mixed setting.

### 2.6 Starting gear

- The ten classes' starting equipment can be randomized.
- Nova's choice is **fully random**: any weapon, armour, talisman or spell,
  usable or not.
- *Proposed:* offer it as an option, with "unchanged" and "random but usable
  by the class's stats" beside it.

### 2.7 Weapon levels and auto-upgrade

- A weapon keeps the upgrade level it has in the unmodded game, wherever it
  lands.
- **Auto-upgrade** (an option, off by default): a weapon the player picks up
  arrives at the highest level they have reached on that upgrade path.
  - *Regular path* (Smithing Stones, +0 to +25): once any regular weapon has
    been raised to +N, every regular weapon picked up after that arrives at
    +N. Raise a dagger to +25 and the next regular weapon found is +25.
  - *Somber path* (Somber Smithing Stones, +0 to +10): kept separately. Only
    raising a somber weapon to +N makes later somber pick-ups arrive at +N.
  - A weapon never arrives below its own unmodded level.

### 2.8 Help for the player

A full **spoiler log** for every seed: where each item went, key items
first. No hints, in game or out.

## 3. Hazards

To be filled in as they are found (our own testing and play). Known so far:
- Placing a key item at a place whose own requirements need that key item
  (prevented by the logic).
- A location the game never actually gives (cut content) receiving an item
  (prevented: such locations are ruled out in the area map).
- **Auto-upgrade depends on what the player has done during play.** A seed
  written before play cannot know the levels, so it needs something that acts
  at pick-up time (event scripts or a native module). Feasibility to be
  worked out before it is promised.

## 4. Required outcomes

- Every seed, under every combination of options, can be finished: the
  checker proves the Elden Beast reachable before the seed is written. In
  separate mode, the DLC's final boss must be reachable too (*proposed*).
- A quest whose items are shuffled can still be finished (§2.4).
- The same seed and options always give the same result, on every platform.
- No item is lost: every item that moves lands somewhere the game gives it.
