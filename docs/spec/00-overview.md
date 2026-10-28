# 00 — Overview

Behaviour specification for an Elden Ring randomizer covering **items** (world pickups, shops, enemy and boss drops, starting loadouts) and **enemies** (bosses, minibosses, regular enemies, NPC invaders). This document set describes *what the program must do* and *why*. It is written for a team that has not seen, and must not see, the reference implementation. Everything here is in our own words. Where a large body of game facts is needed, the spec describes its shape and the facts must be derived independently from the game files (see `10-open-questions.md`).

## Product shape

- A desktop program. The user points it at an installed copy of Elden Ring (base game, optionally with *Shadow of the Erdtree* and the *Tarnished Pack* pre-order content). The user picks options and a seed, then clicks one button.
- The program reads vanilla game data, works out a randomized but **completable** world, and writes a set of modified game files to an output directory. A mod loader loads those files over the game at launch. The program never patches the running process. Optional helper DLLs exist, but they are separate products and not part of this spec beyond their interface (see `08`).
- It also writes a **spoiler log** (a plain-text file of every placement, plus hints) and can export or import an **options file** so that another player can reproduce the same run.

## Two randomizers, one pass

1. **Item randomizer** (optional). It builds a model of every place an item can be obtained ("locations"), every obtainable item, and a logical map of the world (areas plus the requirements to reach them). It places key items first so the game stays beatable, then places everything else by weighted shuffling, then rewrites the game data so each location gives its new item.
2. **Enemy randomizer** (optional). It classifies every enemy instance into a category, builds pools of replacements per category (configurable by the user), assigns replacements subject to placement rules, then rewrites map data, scripts and params so the replacement behaves correctly in its new spot: it fights, it dies, the boss's defeat is recorded, its rewards appear, its health and damage are scaled to the new location, and its name and music change.
3. **Miscellaneous edits.** Some options are independent of both randomizers: starting classes, keepsakes, gestures, NPC outfits, music, colours, convenience fixes, DLC Start mode and the like.

The item randomizer runs before the enemy randomizer and does not depend on where enemies end up. The enemy randomizer can feed information back into item edits, for example moving boss rune rewards with the boss or updating hint text.

## Design goals the reference meets (and we must meet)

- **Never softlock.** If the generator cannot prove a run completable under the selected options, it fails loudly instead of writing files.
- **Missable content never holds required items.** NPC questlines, one-time windows and areas that close off are modelled, and required items are kept out of them.
- **The same inputs give the same output** (see `09`).
- **Merge-friendly.** The program can build on another mod's files instead of vanilla (see `08`).
- **Fail safe at runtime.** If the generated files are only partly installed, the game shows a clear in-game warning instead of silently misbehaving.

## Document map

| File | Contents |
| --- | --- |
| 01-options.md | Every user-facing option, its default, its effect, how options interact |
| 02-item-model.md | Locations, items, item categories, areas, tags, the logic expression language |
| 03-item-placement.md | The placement pipeline: pools, weighting, priority locations, shops, prices, drop rates |
| 04-logic-and-guarantees.md | Key-item logic, completability proof, missable handling, logic options |
| 05-enemy-model.md | Enemy instances, classes, categories, metadata, arenas |
| 06-enemy-placement.md | Pool construction, placement constraints, multi-phase bosses, onslaught, scaling, rewards |
| 07-game-edits.md | Every kind of game file touched and the nature of each edit |
| 08-output-and-merging.md | Inputs, outputs, installation modes, mod merging, failure behaviour |
| 09-determinism-seeds.md | Seeds, sub-streams, what is and is not reproducible |
| 10-open-questions.md | Unknowns, and the bodies of facts the clean team must derive itself |

## Vocabulary (our names)

- **Location**: one place in the game that hands out items. An item lot attached to a pickup, chest, corpse, enemy or event; or one row of a shop.
- **Slot**: a location seen as a container for the randomizer. One location can hold one or more placed items.
- **Area**: a node in the logic graph. It is a named region of the world, such as a legacy dungeon section, an overworld subregion, a mini-dungeon or a boss room.
- **Milestone**: a named logical event that is not an area, such as "boss X defeated" or "Leyndell burned". Areas and items can depend on it.
- **Key item**: an item whose possession opens new areas or milestones.
- **Quest item**: an item that unlocks only a few locations, for example by advancing an NPC questline, and is never required to finish.
- **Priority location**: a location in the set the user marks as "important". In that mode, key and other premium items go only there.
- **Pool** (item side): a partition of items and locations that are shuffled only among themselves, such as finite vs infinite, or base game vs DLC.
- **Enemy class**: the broad category of an enemy instance (regular, boss, miniboss, wildlife and so on).
- **Scaling tier**: the game's own per-area difficulty level for enemies. The game expresses it as a stat-multiplier effect applied to each enemy.
