# 09 — Determinism and seeds

## 1. Seeds

- The **main seed** is an unsigned 32-bit number. The UI fills a random one when blank or when reroll is set. Accept alphanumeric seed strings too, hashed to 32 bits with a stable string hash of our choosing (the reference supports this in a sibling product; adopt it).
- The **enemy seed** is optional. If set and different from the main seed, the enemy randomizer uses it; otherwise it uses the main seed. This lets players share enemies but not items, or the reverse.
- Exported options always include both seeds when they differ.

## 2. Random streams (required behaviour)

Each subsystem draws from its **own** random stream, derived from (seed, fixed per-subsystem salt), so that changing one subsystem does not reshuffle the others:

| Stream | Seeded from |
| --- | --- |
| Key-item and general item logic | main seed |
| Item writing (prices, drop rates, quantities) | main seed + salt |
| Priority-location subset of flask upgrades | main seed + salt |
| Starting classes, keepsakes, NPC outfits | a hash of the **logic options string and the enemy preset content**, so changing logic options changes starting classes but cosmetic toggles do not |
| Gestures, ambient music, spring seals, materials | main seed + per-feature salt |
| Enemy placement | enemy seed, plus a per-class salt for each class's matching |
| Enemy side features (old dragons, boss rewards, threat levels, boss music, scaling choices, forced swaps) | enemy seed + per-feature salt |

The reference uses the platform's standard pseudo-random generator seeded with small integer offsets. **We must specify our own generator** (a fixed, documented algorithm; never the language default, which may change between runtime versions). We must also choose our own salts.

## 3. What is reproducible

- Same program version + same game version + same options string + same seeds + same presets + same merged mod inputs ⇒ byte-identical placement decisions and spoiler log. Output files should also be byte-identical, given deterministic compression.
- Cosmetic and installation options (boss music, ambient music, crash-fix DLL, output mode, merge paths) are **outside** the logic string, and must not change item or enemy placements. Note: boss music *does* use its own stream, so toggling it changes only music.
- Merged mods are part of the input. A mod that adds or removes rows changes the location set and so changes the run.
- Changing the program version may change any run. The options format version is part of the options string so a mismatch can be detected and reported.

## 4. Sources of non-determinism to avoid

- **Map and dictionary iteration order**: all collections the shuffles depend on must be iterated in a defined, sorted order. The reference notes it relies on iteration order in places. We must not.
- **Parallel work**: map loading and writing can be parallel, but any decision that consumes randomness must be sequential and ordered.
- **Unseeded randomness**: the reference uses unseeded randomness for some arena point picks and spawn offsets ("arena points independent of seed"). We should seed these too, from a stream derived from the enemy seed and the target id, so positions repeat for a given seed.
- **Floating point**: weights are floats. Sums and comparisons must give the same results on every platform: avoid fused or extended-precision differences, or use fixed-point.
- **Retry loops**: if generation retries after a failure, the retry seed must come from the original seed, never from the clock.

## 5. Seed validation tooling

The reference has a developer mode that runs key-item assignment over thousands of seeds and reports the failure rate and the distribution of key items across areas. We need an equivalent:
- run N seeds per options profile;
- report the failure percentage, the per-area key-item frequency, and the chain length distribution;
- run the completability simulator (`04` §7) on each.
