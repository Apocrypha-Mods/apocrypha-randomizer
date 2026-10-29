# Item placement bias: behaviour spec

This is the reviewed dirty-room write-up (reviewed 2026-10-29, behaviour only). It describes how the bias slider in the existing Elden Ring item randomizer behaves from the player's point of view. Where it says "public", the information comes from the existing randomizer's public README, mod pages or window text. Where it says "observed", the dirty-room agent worked it out from that randomizer's source and restated it in its own words. Anything marked "unsure" was uncertain in the dirty-room write-up.

## What the player sees

- One horizontal slider in a group labelled "Bias". It runs from 0% to 100% in whole-percent steps and shows its value as a percentage. The default is 20%.
- The bias value is part of the seed's options (it appears in the options string and the spoiler log), so the same seed only reproduces with the same bias.
- Two description lines under the slider change as it moves, one for all items and one for key items. The key-item line is hidden when major key items are not randomized. Window text, quoted, from low to high:
  - All items: "All eligible locations for items are equally likely." (0% only); "Most eligible locations for items are equally likely." (low); "Better rewards for difficult and late locations." (middle); "Much better rewards for difficult and late locations." (high).
  - Key items: "Key items may be in locations that don't require completing much of the game." (low); "Key items will usually be in different areas and depend on each other." (low to middle); "Key items will usually be in different areas and form interesting chains." (middle to fairly high); "Key items will usually form long chains across different areas." (high).
- Public description: the slider ranges from placing "desirable items" "completely randomly" to placing them in "difficult and unfair locations" behind "long item chains". Key items chain together "especially when the bias slider is higher". The README says bias controls "the disparity between weights", from all equal up to very large ratios, and that for key items "the severity of this depends on bias %".
- Public warning (from a sibling game's README, applies in spirit): at high bias, all of a kind of resource item can end up in the final area. The author recommends the "availability similar to base game" style options to avoid this. The Elden Ring equivalent is the "Smithing Stone availability similar to vanilla" checkbox.
- The "unfair location" idea from older games in the series is not used in Elden Ring (observed).

## What bias affects

Bias tilts three separate things, each growing with the slider in its own way.

### Which area key items go to (public and observed)

Covers progression key items (medallions, keys, Great Runes and other items that open areas), plus the other items set to be placed like key items (see below).

- Key items always go where the logic can reach without them. Bigger areas, with more room for key items, are naturally more likely to get one.
- When a key item opens something new, the area it went into becomes less likely to receive further key items, and the areas that the item opens become more likely. The effect spreads key items across areas and builds chains: one key item sits behind another, which sits behind another, and so on.
- How strong this is depends on the slider. At the lowest settings there is effectively no spreading or chaining, so key items often end up early and easy to reach. The effect grows gently through the low range, faster through the middle, and very steeply towards the top. At the top, once an area opens it strongly dominates the next choices, giving long chains deep into the game.
- The response was deliberately shaped so that the default region (around 20%) is fairly mild.
- With otherwise default options there is also a small constant lean towards areas that need more key items to reach, at every bias including 0%. It is off when non-default options or a custom preset are used. (Unsure how noticeable it is at 0%.)
- Fixed per-item placement rules (such as keeping certain items out of certain areas, or keeping base game and DLC separate) always apply on top, and bias never overrides them.

### Which spot within an area gets a key item (observed)

- Key items, and items placed like them, prefer spots that are later within their area and spots that are harder. Very small areas and dead-end spots count as the end of their area.
- Harder means, broadly, spots guarded by bosses or tough enemies, hidden or out-of-the-way spots, ambushes and dead ends, as opposed to plain pickups along the path. A spot with several such traits counts as harder still.
- At 0% every eligible spot is equally likely. As bias rises, the gap between the least and most favoured spots grows very rapidly; at the top it is extreme. The extreme end is somewhat stronger for key items than for ordinary items.
- At the very top of the range the preference for boss-type and dead-end spots eases slightly, so the maximum does not push everything onto bosses. (Unsure of the design intent.)
- Some spots are effectively never chosen for key items: spots that require killing an NPC, early or required bosses that should not hold key items, and bosses whose defeat itself unlocks other progress. Bosses that unlock other progress are treated as middling rather than top-ranked.
- A merchant shop counts roughly as one spot for key items, not one per slot, so shops do not soak up key items.

### Where all other items go (public and observed)

- Applies to every non-key item, separately within each pool (world items, shops, drops and so on).
- More desirable items, and larger stacks of the same item, tend towards later and harder spots; less desirable items tend towards early, plain pickups. How late a spot is reflects how far into the game its area is. Harder uses the same idea as for key items.
- The item-side preference (which items count as more desirable) is fairly gentle and does not change with bias. Bias only changes how strongly late and hard spots are favoured. That grows steadily across the whole range, from all equal at 0% to very strong at 100%, with no flat start.
- Placement rules still win: an item that may not go in a spot goes to the next suitable one.
- At 0% any eligible spot is equally likely to hold any item. At high bias the best items gather in late areas and in boss, hidden or dead-end spots. This is always a tendency, never a guarantee: the README calls it a "fuzzy sort".

### Items placed like key items (observed)

When flask upgrades, Upgrade Bell Bearings, Shadow Realm Blessings and similar items are set to "To important locations", they follow the key-item spot preference above, limited to the location types the player marked important. For Golden Seeds only some of them are handled this way; the rest are shuffled like ordinary items.

## Interactions with other options

- "Major key items randomized?" ("To anywhere" / "To important locations" / "No"). "To important locations" limits key items to the types ticked under "Which locations are important locations?" (major bosses, vanilla locations of major key items, other bosses, merchant shops, Golden Seed trees and Sacred Tear churches, Shadow Realm Blessing pickups, with an option to exclude mini-dungeons). Bias then works only inside that set.
- Flask upgrades, Upgrade Bell Bearings and Shadow Realm Blessings have the same three-way choice.
- "Smithing Stone availability similar to vanilla" keeps upgrade materials close to vanilla availability, countering the high-bias clustering of materials late. (Unsure of the exact behaviour.)
- Logic options (early legacy dungeons, early Mohgwyn Palace, Great Rune counts, Rold Medallion not required, deathless routing, randomizing Lamenter's Gaol keys, separate base game and DLC key items, Messmer's Kindling counts) change what is reachable, so they change what counts as late and what chains are possible. Their fixed placements always win over bias.
- With base game and DLC randomized separately, each is weighted on its own, with its own early and late. (Unsure.)
- Custom placements and presets can pin or restrict items and always override bias. Using a preset also turns off the small key-item lateness lean.
- Hints (buying map markers from Kalé) are not affected by bias.

## What the player can rely on

- The game can always be finished at every bias: a key item is never placed behind itself.
- Bias only changes likelihoods. It never breaks logic, important-location limits, base game and DLC separation, or custom placements.
- At 0%: non-key items are equally likely anywhere eligible, and key items are spread only by available room (apart from the small default lateness lean).
- At 100%: no spot is ruled out by bias alone; even the least favoured keeps a small chance. Results are still heavily skewed: long key-item chains across many areas and the best rewards at late, hard spots, with a risk of resource items bunching in the final areas.
- Same seed and same options, bias included, give the same placement; changing bias changes it.

## Open questions

- Where exactly the description lines change wording along the slider (only rough positions are known).
- How steep the response should be at each part of the range, in numbers; only the shape is known. We will need to choose our own curve and check it by play.
- How strong the most extreme setting should be in practice (the public README speaks only of "very large ratios").
- How noticeable the small default lateness lean is at 0%, and whether we want one at all.
- Why the top of the range eases off boss-type spots, and whether we want the same.
- How finely spots should be graded by difficulty and in what order; only the broad idea is known, so we design our own grading.
- Exactly what "Smithing Stone availability similar to vanilla" does alongside bias.
- How weighting works when base game and DLC are randomized separately.
