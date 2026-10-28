# 03 — Item placement pipeline

## 1. Overview of the pass

1. Load the item model (`02`). Apply options: remove items, add new items, mark not-randomized items and locations, compute dynamic logic.
2. Partition everything into **pools** (below). Items shuffle only within their pool.
3. **Key-item phase** (`04`): give every key item a set of allowed *areas*, so completability holds whatever happens next.
4. **Quest-item phase**: give each quest item an allowed area, with looser rules.
5. **Premium phase**: fill priority locations with key items (now resolved to concrete locations in their assigned areas) and the premium classes the user sent to priority locations.
6. **General phase**: weighted shuffle of the remaining items into the remaining locations, with per-item restrictions.
7. **Leftovers**: items that could not be placed under preference rules go anywhere still legal. If a required item cannot be placed legally, generation **fails** (see `04` §6).
8. **Write** (`07`): rewrite lots and shop rows; compute prices and drop rates; rewrite scripts that check for moved items.

## 2. Pools

Locations and items fall into availability pools:

| Pool | Contents | Rule |
| --- | --- | --- |
| Finite | One-time pickups, finite shop stock, guaranteed one-off drops | Main shuffle. All key and quest items live here |
| Infinite | Infinite shop stock, repeatable random enemy drops | Shuffled among infinite items, so infinitely buyable items stay infinite and farmable stays farmable |
| Mixed | Shop stock that has both a finite and an infinite variant (a few copies, then infinite after an unlock) | Kept consistent: both variants of a row must carry the same item |
| Self | Items that must map to themselves for softlock or balance reasons | Not moved, but still written (so flags stay consistent) |
| Removed | Items an option deletes | Location filled with a filler item or emptied |

Pools are further split by **scope** when the user asks:
- base game vs DLC (for all items, or only key items);
- custom item pools (for example, "all weapons shuffle among weapon locations by weapon type").

Accounting rule: within a pool, the number of target locations must be at least the number of source items. When a vanilla location shared a base slot across pools (one lot handing out items from two pools), extra capacity must be planned so neither pool overflows.

DLC content not owned, or not enabled, is removed from both sides of every pool. Locations in areas that are logically unused (DLC turned off) are disabled.

## 3. Weighting

The general phase is a weighted shuffle, not a uniform one:

- Each **item** gets a weight from its desirability tier: highest for legendary or unique gear, lower for ordinary consumables. Items listed earlier in the priority ordering weigh more.
- Each **location** gets a weight from:
  - **lateness**: how deep into the logical game it is (fraction of all items reachable before it, computed from the logic graph after key items are placed);
  - **difficulty**: boss rewards, out-of-the-way spots, and locations needing an NPC to be killed;
  - **demotions**: locations that require killing an NPC, early mandatory bosses, and milestones that unlock other things anyway are pushed down;
  - **shop aggregation**: a big merchant inventory gets roughly the total weight of one location, so shops do not soak up all good items.
- Both lists are sorted by a **weighted random permutation**: repeatedly pick an element with probability proportional to its weight. Then the two lists are **zipped**. The i-th item goes to the i-th location if that is allowed; otherwise it waits in a queue, and each new location first tries the queued items.
- The spread between the highest and lowest weight grows with the progression-spread option: equal at 0, then exponential up to a large ratio at 100 (*exact exponents tuned upstream, choose our own*).
- Within one area, key items use their own spread so they do not always land on the single "hardest" location in an area.
- Ring and talisman tiers (+1, +2, +3 of one family) are post-sorted so a weaker version is not found after a stronger one, where possible.

## 4. Priority locations

- The priority set is built from the groups the user ticked (`01` §3.2), minus excluded mini-dungeons, minus missable locations, and minus any location that is risky for important items (regular-enemy drops; also any location that "could" be removed by other options).
- Premium classes set to "priority locations" may go only into this set. Key items set to "priority locations" must go into the set *and* their logic-assigned area.
- If there are fewer priority locations than items that must go there, the program **adds priority capacity** from a fallback list of good locations, in a deterministic order. If that is not enough, it fails with a message that says to turn on more location groups.
- Golden Seeds are counted against the expected number of flask upgrades. If only some can fit (more seeds than spots), a fixed subset is placed at priority locations by a separate random stream. The rest go anywhere.
- Shops in the priority set should stock a premium item with quantity 1. A shop row with infinite stock must never hold a one-off premium item.

## 5. Restrictions applied during placement

A placement of item I at location L is legal only if all of these hold:

1. L is in I's pool and scope.
2. L's tags do not exclude I (missable excludes key and quest items; "exclude X" tags; DLC-only items need DLC locations if scoped).
3. If I has a logic-assigned area set (key and quest items), L is in one of those areas, or L's milestone is in one of them.
4. I's lateness restrictions (no earlier or later than X) hold for L's area.
5. Custom rules: required tags, "never in" sets, premium-location item filters.
6. Shop sanity: key items in shops are sold once; premium items are not placed in infinite rows; at most one copy of the same restricted item per shop.
7. Special synthetic items, such as a "Path of the Dragon" style scripted grant, must be attached to an event-type location.
8. Do not stack two key items in one location unless `multi_key_per_slot` is on.

## 6. Shop prices

Prices are rewritten for randomized shop rows:

- Prices are drawn at random from the vanilla prices of **similar items** (same category or class), so a talisman is priced like talismans and an arrow like arrows.
- Key items get a separate price band. The ceiling stays moderate so a required key item is never priced out early. Some items have hand-set sanity caps (very high vanilla prices on key-ish goods are not copied).
- Upgrade materials stay near vanilla. Items moved into shops in large numbers (unique materials) get a discount.
- Non-randomized shop rows keep their vanilla prices.
- Rows that cost a currency other than runes (remembrance and dragon-heart exchanges) keep their currency and cost semantics.

## 7. Drop rates

When an item is placed in a random (non-guaranteed) enemy drop:
- The new chance combines the vanilla chance of that drop slot with the item's class (equipment is rarer than consumables) and an enemy-difficulty factor.
- Quantities are drawn from vanilla quantities for the item's category.
- Guaranteed drops stay guaranteed.

## 8. Upgrade levels

- Randomized weapons placed in late locations may be pre-upgraded to match the vanilla upgrade level that location would have offered. With `dlc_max_weapons`, every DLC weapon location gives a max-level weapon. With DLC Start plus a "level" setting, weapons scale to the expected character progression.
- Remembrance-exchange locations (boss soul shops) can upgrade their randomized output when the run implies late access.

## 9. Spoiler log and hints

- The log lists every randomized location with its new item, grouped by area in rough game order, then a hints section. The hints say which areas hold each key item, which key items are actually required for the ending, and an approximate order.
- In-game hints (optional): Kalé sells marker items. Using one places map markers at the areas, or exact spots, holding required items. Markers can also show every unchecked priority location (see `07`).
