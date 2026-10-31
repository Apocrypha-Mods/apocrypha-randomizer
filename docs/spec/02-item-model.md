# 02 — Item model

## 1. Items

- An **item** is identified by (category, numeric id). The categories are the game's own equip categories: weapon (including ammunition), protector (armor), accessory (talisman), goods (consumables, key items, spells, spirit ashes, materials and so on), gem (Ash of War), and custom weapon (a pre-configured weapon plus affinity plus Ash of War row).
- Upgraded weapon variants (+N) and infused variants share a base. The model normalizes them to the base for placement and keeps the level and infusion as quantity-like data at the location.
- Items need display names in every supported language. They come from the game's text (FMG) files. The program also needs a stable human name per item for logs and presets, independent of language.
- **Item classes used by placement.** Each item gets: a desirability rank (see `03`); finite or infinite availability (one-off vs sold infinitely or farmable); whether it is a key item, quest item, premium class (flasks, bell bearings, DLC blessings), crafting material, spell (sorcery or incantation), spirit ash, consumable, ammunition and so on; and whether it is base game, DLC or pre-order content. Base vs DLC is decided by id ranges or param metadata plus a short exception list. Some DLC items interleave with base ids.

## 2. Locations

A **location** is one way the game hands out an item. Kinds:

1. **Map item lots** (`ItemLotParam_map`). Each is reached through one or more *carriers*: a treasure on a map asset or corpse (MSB treasure events), a scripted award in an event script (EMEVD "award item lot" commands), or an NPC talk script (ESD) awarding a lot. A lot row can chain to the following rows to form a multi-item lot. Each lot has an acquisition **event flag** that marks it collected.
2. **Enemy item lots** (`ItemLotParam_enemy`). Drops referenced by an enemy's NPC param row or by its map placement. These include guaranteed one-off drops (a flag is set) and random repeatable drops (no flag, a drop chance). Boss drops are usually scripted map lots awarded on the defeat flag, not enemy lots.
3. **Shop rows** (`ShopLineupParam`, plus the recipe-book variant for crafting). Each row is an item, a price, a quantity (finite or infinite), and an optional event flag that tracks purchases or unlocks the row. NPC talk scripts open shops by **row-id range**, so a merchant's inventory is a contiguous id range. Some ranges unlock only after a flag (bell bearings, quest progress).
4. **Starting equipment** (`CharaInitParam`) for classes, and keepsake gifts. These are handled by the starting-class logic, not the main shuffle.
5. **Special and scripted grants**: items given directly by scripts without a lot (a gesture pickup, items handed over in dialogue, Great Rune "restoration"). These need custom handling: either they are left alone, or a fake lot or shop row is synthesized so they can be randomized.

A location is identified by a **location key** stable across game versions: its kind, its lot or shop row id, its event flag, and the identity of its carrier(s). The same lot can be referenced from several carriers. For example, the same merchant appears in two places, or an NPC moves. Such carriers merge into one location with several places it appears.

### Location discovery (scraping)

The program finds all locations itself by reading game data:

- Scan every map's MSB for treasure events and enemies with drop lots; scan every map's EMEVD for item-lot awards (direct, or through common event templates with lot or flag parameters); scan every talk ESD for item-lot awards and shop-range openings; scan `ShopLineupParam` in full; scan `NpcParam` for enemy drop lots.
- Group each location by its **map** (the world tile or dungeon id) and, for overworld tiles, by approximate position, so it can be placed in a logical area.
- Locations that cannot be acquired in game (unused lots, debug shops) are excluded. Locations not in the annotation data are, by default: not randomized if they are shops, and treated as missable if they are lots.

### Location attributes (the annotation layer)

Each meaningful location has a hand-written annotation (the reference ships about 1,900 base-game and DLC entries). The schema we need:

| Field | Purpose |
| --- | --- |
| location key | Matches the scraped location |
| area | The logic area the location is in |
| full-area list | The areas that can be passed through to reach it (used for hints and markers) |
| milestone | Optional. The location is granted at a logical event (such as a boss defeat), not just by being in the area |
| quest requirements | Optional. Logic expression: other milestones or items needed besides the area, for example "after NPC X's questline step" |
| text | Human description for the spoiler log and hints, in our own words |
| tags | Set of attributes, below |

Tag vocabulary (our names; the reference has a similar set):

- **missable**: can be permanently lost (NPC questline, one-time event, area that closes, NPC that can die or leave). Required items are never placed here.
- **boss reward / major-boss reward / minor-boss reward**: drop from a boss with a health bar. Used for priority location groups and to deprioritize early required bosses.
- **enemy drop** (with sub-kinds by item type): drop from a regular enemy. Treated as risky for important items, especially under enemy randomization.
- **chest**, **corpse**, **statue-locked** (behind an imp seal), **painting**, **night-only**, **seed tree**, **church**, **evergaol**, **caravan**, **merchant gift** and so on: descriptive, usable in custom rules.
- **shop** sub-tags: spell shop (sorcery or incantation), dragon-heart shop, boss-remembrance shop, ash-of-war shop, cookbook, bell-bearing shop, premium shop.
- **mini-dungeon**: catacomb, cave, tunnel, hero's grave, divine tower and so on (for the "exclude mini-dungeons" option).
- **out of the way**: hard to find. Used in weighting.
- **dead end**: obtaining it leads nowhere. Used for chaining heuristics.
- **norandom**: never randomized. Examples: the starting tutorial item, items that drive scripts that must not change, and items the game hard-checks.
- **exclusive**: one of several mutually exclusive choices. Only one can be obtained, so it is treated as missable.
- **excludes for specific items**: "item X can never go here". Example: a location that also needs X to reach, or an item whose script would break at this location.
- **mod-only / DLC-only / pre-order-only**: content gating.
- **fog-change**: interacts with the fog-gate compatibility mode.

### Multi-item locations and merged events

- Some locations hand out several items. In vanilla, each item can be a separate placement target (one per lot entry), or the whole set can be one target. Default: one placed item per vanilla entry. With `multi_key_per_slot` off, a single location never holds more than one key item.
- Some groups of locations share one event flag, such as a chain of lots or shop rows. They must be rewritten together and stay consistent.

## 3. Areas and milestones

- The world is a directed **logic graph** of areas. Each area has a name, a display name, the map ids it covers (and for overworld tiles, which subregions), an optional **parent or key-base** (for grouping heuristics), descriptive tags (minidungeon, cave, legacy dungeon, overworld, DLC) and a **requirement expression**.
- **Milestones** have a requirement expression too, and usually sit inside an area.
- The reference graph has a few hundred areas and milestones. It must be authored independently from game knowledge (see `10`).

### Requirement expression language

Boolean expressions over names:
- an area name (that area is reachable);
- a milestone name;
- an item's logical name (the item is held);
- `AND`, `OR`, parentheses;
- **counted** terms: "at least N of {item set}", used for Great Runes and shards;
- **option switches**: a sub-expression that applies only when an option is on or off. For example, "if `altus_needs_dectus`, then Dectus is required for this route";
- **macros**: named shared sub-expressions defined once.

Two time qualifiers exist on locations, not on areas:
- **until**: the location is available only *until* a named milestone (it becomes missable after that). For example, before the capital burns.
- **after**: the location becomes available only after a milestone.

### Dynamic parts filled in from options

Some requirements are computed from options, not authored:
- "enough Great Runes to enter Leyndell" becomes a counted expression over a chosen subset of runes (see `04`);
- the Mountaintops gate becomes Rold or N runes;
- region-lock items become extra requirements on every area of a region;
- shard counts replace single items;
- linked imbued keys put a specific key on each sending gate.

## 4. Item groups

Named item groups the program needs (contents to be derived independently):
- **key items** (affect area access);
- **quest items** (unlock only locations);
- **premium classes**: flask upgrades, bell bearings that unlock smithing-stone shops, DLC blessings;
- **remove group**: items removed by options (Rold when replaced by runes, crafting kit contents with all-crafting on and so on);
- **norandom group**: items logic depends on that must stay put;
- **priority ordering groups**: ordered tiers of item desirability (legendary items, rare talismans, notable spells, ordinary consumables and so on) used to weight placement;
- **starting groups**: candidate pools for class loadouts and keepsakes (see `07`).

## 5. Placement restrictions per item

Besides tags, an item can carry **placement restrictions**:
- "must be placed no later than milestone/area X" and "not before X". Used so smithing stones keep a vanilla-like curve, and certain spells or tools stay early;
- "must appear at least K times in these areas" (distribution requirements for stackable progression items like smithing stones);
- "one of these areas" lists;
- bell bearings: keep each smithing-stone bell bearing roughly in its vanilla region band.

These are soft data, authored per item; derive our own.

## 6. New items

The program can **add items that do not exist** in vanilla: region-lock keys, master Stonesword Keys, kindling shards or other shards, trap items, and item copies for `add_drop_copies`. Each new item needs a new goods row (copied from a template row, with an icon and sort id), new names and descriptions in every language, and (if it is a key item) logic hooks and script checks. See `07`.
