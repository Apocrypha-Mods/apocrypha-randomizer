# 07 — Game files read and edited

All of these are facts about the game's file formats and data tables. Each entry lists the file kind, what is read, and the nature of the edits.

## 1. Params (`regulation.bin`)

The regulation file is an encrypted, compressed bundle of param tables. It is read in full and written back with edits. Tables touched, and why:

| Table | Read for | Edits |
| --- | --- | --- |
| `ItemLotParam_map` | Map pickup and scripted lots | Rewrite item category, id, count and chance fields per lot entry. Rewrite the lot's acquisition flag where a moved key item needs its own flag. Add new lot rows for new locations and synthesized carriers |
| `ItemLotParam_enemy` | Enemy drops | Rewrite items, chances and quantities. Add lots for enemy variants |
| `ShopLineupParam` | Merchants | Rewrite item, item category, price, quantity and event flags. Add rows for synthesized shops (base-game shop in DLC Start, marker-hint shop at Kalé) |
| `ShopLineupParam_Recipe` | Cookbook-unlocked crafting | Unlock all (option); keep consistent with moved cookbooks |
| `EquipParamGoods` | Goods metadata | New goods rows for new items (region locks, master keys, shards, traps, hint markers). Change max counts and "show quantity" for shard items. Change icon and sort ids. Key-item flags. Make fake-item copies unusable |
| `EquipParamWeapon`, `EquipParamProtector`, `EquipParamAccessory`, `EquipParamGem`, `EquipParamCustomWeapon` | Equipment metadata, requirements, rarity | Remove stat requirements (option). Make a weapon unupgradeable (option). Read stat requirements for starting-class fitting |
| `EquipMtrlSetParam`, `ReinforceParamWeapon` | Upgrade costs | One stone per level for regular upgrades (option) |
| `CharaInitParam` | Starting classes and NPC equipment | Rewrite class weapons, armor, spells, stats and items. Rewrite NPC armor (outfits). Add NPC equipment rows for moved humans |
| `NpcParam` | Enemy stats, drops, teams, effects, threat level, names | Create variant rows (boss-as-regular, regular-as-boss). Set rune amounts, teams, no-death flags, threat levels, resistances and resident effects. Point at new drop lots |
| `NpcThinkParam` | AI | Copy or alter think rows: night-time checks, return-home behaviour, perception |
| `SpEffectParam` | Status effects | Create scaling effects (one per tier pair), phase health adjustments, nerfs (Malenia heal, gargoyle poison tick, Aging Untouchables), trap effects, resident effects for helpers, colour tint effects |
| `SpEffectVfxParam` | Effect visuals | Tint and visibility effects (rainbow, hidden helpers) |
| `AtkParam_Npc`, `AtkParam_Pc` | Attacks | Small fixes (specific nerfs) |
| `GameAreaParam` | Boss-area rewards and metadata | Rune rewards when bosses move. Aesthetic counts |
| `BonfireWarpParam`, `BonfireWarpSubCategoryParam` | Grace warping | Unlock or add graces for DLC Start and convenience. Mark graces for hints |
| `WorldMapPointParam`, `WorldMapLegacyConvParam` | Map markers | Add hint markers (areas or exact locations of required items; unchecked priority locations). Unlock icons |
| `AssetEnvironmentGeometryParam` | Asset behaviour | Make specific blocking assets breakable or unbreakable; region-lock walls |
| `GestureParam` | Gestures | Shuffle gesture rewards. Make the gesture item consistent with its grant |
| `BuddyStoneParam`, `PhantomParam`, `ObjActParam`, `WireSetParam`, `LockCamParam`, `CameraSetParam`, `ChrActivateConditionParam`, `CutSceneTextureLoadParam`, `MaterialExParam`, `MenuValueTableParam`, `TalkParam` | Misc | Targeted fixes for moved enemies (spirit summon stones near bosses, phantom colours, lock-on cameras, activation conditions, talk text references) |
| `WwiseValueToStrParam_BgmBossChrIdConv` (and the env-place variant) | Music | Boss and ambient music shuffling |

**Provenance**: the program writes its options string into an unused text field of the params, so a generated regulation identifies its own options. It also writes a checksum of the params it needs, used by the in-game installation check (§6).

## 2. Event scripts (EMEVD, `event/*.emevd.dcx`)

- **Read**: every map's script plus the common and common-function scripts. They are used to find item awards, boss encounter structure, flag usage, and event templates with typed arguments. A full **instruction definition table** (bank, id, argument types and names) is needed to read and write instructions safely. It must be built independently from public community documentation of the format.
- **Edits on the item side**:
  - Rewrite **"has item" and "flag" checks** that refer to moved items. Doors, lifts, gates, NPC triggers and the Leyndell rune gate check either the item's acquisition flag or item possession. When the item moved, the check must use the item (or a new flag set when the item is obtained), not the old location's flag.
  - Add events that **set a flag when an item is obtained** (polled possession checks) for items whose vanilla flag no longer works. These run once on load to resync.
  - **Great Rune flags**: set the rune-held flags from possession, not location. Rewrite the rune-count flag used by the Leyndell gate to the requested count. Add a final-boss rune gate.
  - **Rold replacement**: change the Grand Lift check to a rune count.
  - **Logic gates**: new events that block or unblock routes (region-lock walls, Liurnia and Altus blockers, Dragonbarrow lock, Stormveil gate) on flags or items.
  - **Shard checks**: replace "has item X" with "has ≥ M of shard X".
  - **Duplication helpers**: remembrance and Ash of War duplication must depend on having obtained the item, not on the vanilla boss flag.
  - **Synthesized grants**: events that award newly created lots, for gesture-like or scripted items turned into normal pickups.
  - **Royal Capital after Maliketh** (option): stop the script that turns off the capital's access flags. Disable the Outskirts elevator in the ashen state, close a sewer door whose fog does not survive save-and-quit, and move the elevator.
  - **Rya, Ymir and similar** quest softlocks: targeted script edits so quests do not hang when an item moved.
  - **Traps**: events that apply harmful effects while a trap item is held, then remove it.
  - **Spiritspring seals** (option): change which spring each seal unlocks, and the message.
  - **Kalé marker hints**: events that show markers when a marker item is used.
- **Edits on the enemy side**:
  - Re-target boss encounter events to the new enemy (fog, bar, music, defeat flag, reward award).
  - Transplant the source boss's own events into the target map with ids, regions and flags remapped.
  - Add events for onslaught clones (wait for all deaths).
  - Night-boss gating, threat level and music flag edits.
  - Remove or neutralize events for removed helpers (prefer turning them into no-ops over deleting them, to keep script indices stable).
- **Misc**: DLC Start setup and warp events, grace unlocks, a resident failsafe event that shows the mismatch message (§6), and a runtime check that warns if DLC content is used without the DLC installed.

## 3. Event flags

- Item acquisition flags, shop purchase flags, boss defeat, appear, start and music flags, and the rune flags are all moved or rewired as described above.
- New flags are needed for: new item checks, external-item checks, logic gates, region locks, DLC Start state, grace unlocks, temporary script state, and enemy randomizer bookkeeping. They must come from **flag ranges the game treats as valid and persistent** (or temporary where that is intended) and that vanilla does not use. Choose our own ranges by inspecting the game's flag-block layout.

## 4. Maps (MSB, `map/mapstudio/*.msb.dcx`)

- **Read**: every map, for enemies, treasures, assets, regions, collisions, platoons and generators.
- **Edits**:
  - enemy parts: model, params, position, groups, collision name, initial animation, entity ids;
  - new parts: clones (onslaught), helpers, regenerated platoons and generators;
  - regions: new or moved points for arenas, spawn points and AI regions;
  - treasures: point at new lots where carriers are synthesized;
  - assets: region-lock walls (new invisible blocker assets at region borders), Mountaintops shortcuts, Abyssal Woods Torrent edits, dismount points, and cosmetic changes;
  - sound regions: ambient music shuffling;
  - for low-detail duplicate tiles: mirror edits, or delete the duplicate.
- Only maps that actually changed are written. Maps written by a previous run but not this one must be **removed** from the output, so stale edits do not linger.

## 5. Text (FMG in `msg/<lang>/item*.msgbnd.dcx`, `menu*.msgbnd.dcx`, including DLC variants)

- Item names, descriptions and captions for new items in every language.
- Boss names (renames and mashups), NPC names.
- Starting-class descriptions. Keepsake descriptions.
- Event text (pop-up messages: gate locked, shard count, mismatch warning, DLC-missing warning, seal messages).
- Map place names for hint markers.
- Localization: the program has its own translations for its messages, kept in per-language message files. Languages without a translation fall back to English.

## 6. Talk scripts (ESD, `script/talk/*.talkesdbnd.dcx`)

- **Read**: all talk scripts, to find shop openings (row-id ranges), item awards, item and flag checks, and quest conditions.
- **Edits**:
  - replace "has item" or "flag" checks with possession or shard-count checks (dialogue branches that gate on Rold, Great Runes, quest items);
  - Gostoc's gate option (Rusty Key requirement);
  - Kalé's extra shop for hint markers;
  - Rya's "take my hand" teleport (quest gating);
  - merchant bell-bearing unlocks: when a bell bearing is randomized, the talk script that adds its shop rows to the Twin Maiden Husks must still work, and the award moves with the item;
  - remove talk for moved enemies where it breaks.

## 7. Other assets

- **Character animation (TAE)**: one known fix for a specific dragon variant.
- **SFX bundles**: standalone visual-effect bundles for bosses whose effects live only in their home map, copied so the effects work elsewhere.
- **AI scripts (Lua)**: generally not edited in Elden Ring. Hard-coded region references are handled by moving regions instead.

## 8. Starting classes and gifts (behaviour)

- For each class: choose a weapon (and shield, catalyst or seal, and spells when the class casts), weighted to prefer items whose stat requirements are close to the class's stats. Requirements gaps can be fudged by small stat increases (option), capped so no stat goes far above a sane early value, and keeping the total level near vanilla.
- One-handable vs two-handed vs unwieldable filters per option. DLC-only filter.
- Armor: random sets generated per slot.
- Class descriptions updated per language.
- Keepsakes: one random item per vanilla keepsake slot, from category buckets (talisman, spirit ash, key-ish tool, buffs, runes, health, throwables, misc), with sanity limits (no huge rune amounts; cheap ashes).
- With DLC Start: care package, initial levels, given upgrades.

## 9. Misc fixes always or optionally applied

- Nerfs listed in `01` §3.10; Torrent fear removal (option); Serpent-Hunter tied to the Meteorite Staff upgrade path or made unupgradeable.
- Some "always" safety edits even without randomization, for example making specific broken NPCs non-hostile. Derive our own list from testing.
