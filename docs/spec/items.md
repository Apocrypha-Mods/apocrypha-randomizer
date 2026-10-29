# Item randomizer: behaviour spec

Written 2026-10-29 from Nova's own design decisions (route C in the spec
plan, item half). No upstream material was read to write it, so it needs no
contamination review. Where a detail is still open it says so; nothing here
is our own suggestion.

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
- What exactly moves in a seed is the player's choice, not ours. Every
  category of item has its own **shuffle slider**, 0–100%:
  - at 0% the category stays exactly where the unmodded game has it;
  - at 100% all of it moves;
  - in between, each item of it moves with that chance, drawn from the seed.

  Items that move trade places among the slots of the items that moved.
- The categories, read from the game's own data:
  - key items: what any requirement of the area map names;
  - weapons;
  - armour;
  - talismans;
  - spells (sorceries and incantations);
  - Ashes of War;
  - spirit ashes;
  - upgrade materials;
  - quest and tool items: other important goods such as keys, letters,
    cookbooks and bell bearings;
  - consumables and crafting materials.

### 2.2 Merchants

- Shop stock can be shuffled with everything else.
- How random the shops are is the player's choice: a **merchants** shuffle
  slider, 0–100%, for how much of their stock is put in play.
- Pricing is open. Until it is decided, a shop row keeps its own price and
  stock, whatever item it now sells.
- Rows paid for in items (Dragon Communion, remembrance trades) stay as they
  are.

### 2.3 Where things land: the player's bias

- Key items may go **anywhere the logic can reach in time**. Nothing is ruled
  out by us.
- Every category also has a **bias slider**, 0–100%: how strongly what
  moves leans. At 0% every free slot is equally likely.
- **Depth.** Bias leans on how deep a slot is, from 0 to 1. It is our own
  measure:
  - mostly *lateness*: how far into the game its area opens, in the
    unmodded game's order;
  - partly *hardness*: boss rewards are hardest, then one-time drops from
    notable enemies, then dungeon items, then NPC gifts and scripted awards,
    then items lying in the open world.
- **Key items** lean towards deep slots. The higher the bias, the more key
  items sit behind other key items, forming chains.
- **Other items** lean towards slots as deep as they are worth. An item's
  worth is how deep the unmodded game hides it, so late-game items drift
  late and early items early.
- The slider's effect grows with its square: gentle at the low end, strong
  at the top. At 100% no slot is ruled out, it is only much less likely.
- How the sliders behave was informed by how players already know such a
  slider to behave (`bias.md`, a reviewed behaviour-only study). The design
  here is our own.

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

The DLC counts by map: Shadow of the Erdtree's open world and its own legacy
dungeons, catacombs, gaols, forges and pits. Starting gear and gifts with no
map count as the base game.

### 2.6 Starting gear

- The ten classes' starting equipment can be randomized.
- Nova's choice is **fully random**: any weapon, armour, talisman or spell,
  usable or not.
- A **starting gear** shuffle slider, 0–100%, sets how much of it is put in
  play.
- Each slot takes an item of its own sort: weapons for weapon slots (not
  arrows or bolts), armour for the same body part, talismans, and spells.
- The other starting goods, such as the flasks, stay as they are, and key
  items never start in a class's hands, since the player's class is not
  known in advance.

### 2.7 Weapon levels and auto-upgrade

- A weapon keeps the upgrade level it has in the unmodded game, wherever it
  lands.
- **Auto-upgrade** (an option the player can turn on): a weapon the player picks up
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
  written before play cannot know the levels, so it is done by a native
  module of ours that me3 loads with the mod and that acts at pick-up time.

## 4. Required outcomes

- Every seed, under every combination of options, can be finished: the
  checker proves the Elden Beast reachable before the seed is written.
- A quest whose items are shuffled can still be finished (§2.4).
- The same seed and options always give the same result, on every platform.
- No item is lost: every item that moves lands somewhere the game gives it.
