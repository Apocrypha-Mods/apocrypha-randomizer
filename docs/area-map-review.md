# Area map: what to check in play

The randomizer only puts a key item (a medallion, a Great Rune, the Academy Glintstone
Key, Messmer's Kindling and so on) where its area map says the player can get there.
That map is `crates/logic/data/elden_ring.toml`. Every claim marked as unsure in the
first draft was settled on 2026-10-31 from the game's scripts and public guides, but
only eight have been seen in play. This list is the rest, to tick off during a play-through.

**Which mistakes matter.** If the map says a way is **open** when the game actually
locks it, a seed can be unfinishable: the randomizer could put the Rold Medallion
behind a door that needs the Rold Medallion. If the map **asks for more** than the
game does, nothing breaks: keys just have fewer places to go. So section 1 is the one
that matters, and later sections only affect how much variety seeds get.

When something is wrong, note what you found (for example "the Divine Tower of Caelid
needed X"), and the fix is a line in the data file.

## 1. Ways the map says are open (a mistake here can break a seed)

Check that each of these really needs nothing: no key item, no boss, no quest step.

**Base game**
- [ ] Limgrave → Caelid, and Caelid → Dragonbarrow: open on foot.
- [ ] Dragonbarrow → Divine Tower of Caelid: no item or boss needed to get in and to the top.
- [ ] Limgrave → Siofra River (the well in Mistwood): open.
- [ ] Nokron → Deeproot Depths: open through Night's Sacred Ground and the aqueduct, once Nokron is reached.
- [ ] Academy of Raya Lucaria → Volcano Manor: the Abductor Virgin's grab works without
      any quest step or item.
- [ ] Liurnia → Caria Manor, and Liurnia → Ainsel River (the well): open.
- [ ] Liurnia → Ruin-Strewn Precipice: open (Magma Wyrm Makar is asked for only to go on to Altus).
- [ ] Altus → Mt. Gelmir → Volcano Manor: open, with no quest step.
- [ ] Leyndell → Subterranean Shunning-Grounds: open.
- [ ] Leyndell → Divine Tower of West Altus, through the Sealed Tunnel under the
      Fortified Manor: open.
- [ ] Forbidden Lands → Divine Tower of East Altus → Isolated Divine Tower: open
      (the map already shuts the East Altus tower once the Erdtree burns).
- [ ] Consecrated Snowfield → Haligtree: only Ordina's evergaol puzzle, no item.
- [ ] The Roundtable Hold: Melina brings you after enough graces or at Stormveil's gate,
      with nothing to hold.

**Shadow of the Erdtree**
- [ ] Gravesite Plain → Belurat, Castle Ensis and the Cerulean Coast (down the Ellac River to its cave): open.
- [ ] Scadu Altus → Rauh Base, through the tunnel from Moorth Ruins: open.
- [ ] Scadu Altus → Shadow Keep: open by the main gate, or the Church District by way of
      Bonny Village. In particular: does the Golden Hippopotamus have to die before you
      can go deeper into the Keep? No source says, so the map asks nothing.
- [ ] Shadow Keep → Ancient Ruins of Rauh and the Church of the Bud: from the Storehouse's
      West Rampart, over the long bridge and up the lift, with nothing needed. Also: is
      there a way from these ruins to Rauh Base? The map treats them as two separate places.
- [ ] Shadow Keep → Hinterland: the lift from the Storehouse's Loft to the Back Gate, then
      the O Mother gesture at the statue. The gesture comes from Bonny Village and isn't
      shuffled, so the map asks for nothing.
- [ ] Shadow Keep → Abyssal Woods: the painting room, the coffin to Castle Watering Hole,
      then the Darklight Catacombs and Jori. The map doesn't ask for Jori himself.

## 2. Which area a grace belongs to (a mistake can put a key in the wrong place)

Items go to the area of their nearest grace. If a grace is in the wrong area, the items
around it are judged reachable when they're not, or the other way round.

- [ ] Behind Caria Manor: counted with the Three Sisters, so it's past Loretta.
- [ ] Royal Moongazing Grounds (Loretta's arena): counted on Caria Manor's side, before her.
- [ ] Great Waterfall Basin: counted in Nokron (behind Radahn), not open Siofra.
- [ ] Deep Siofra Well: counted in Caelid, not Dragonbarrow.
- [ ] Capital Rampart: counted outside Leyndell's walls, on the Altus Plateau.
- [ ] Wyndham Catacombs: on the Altus Plateau.
- [ ] Craftsman's Shack: on Mt. Gelmir.
- [ ] Church of Repose: in the Mountaintops of the Giants.
- [ ] Stormhill: folded into Limgrave, as nothing gates it.
- [ ] Dragon's Pit: counted with the Gravesite Plain.
- [ ] Viaduct Minor Tower: with the Ancient Ruins of Rauh. Ravine North: with Rauh Base.
- [ ] Forsaken Graveyard and Church Ruins: with the Abyssal Woods. The m40_02 map tile and
      Castle Watering Hole: with the Shadow Keep.

## 3. What a gate asks for (a mistake only narrows where keys go)

These ask for a boss or an item. Check that the game asks for it, and for nothing more.

**Base game**
- [ ] Stormveil itself needs Margit.
- [ ] Divine Tower of Limgrave: the bridge from the castle's ramparts, after Godrick.
- [ ] Nokron needs Radahn (the falling star).
- [ ] Raya Lucaria needs the Academy Glintstone Key.
- [ ] Three Sisters need Loretta.
- [ ] Divine Tower of Liurnia needs the Carian Inverted Statue (Carian Study Hall flipped).
      Ranni gives the statue for the Fingerslayer Blade.
- [ ] Chapel of Anticipation, via the Four Belfries, needs the Imbued Sword Key.
- [ ] Moonlight Altar: through the coffin at the Grand Cloister and past Astel.
- [ ] Altus: either Makar (by the Precipice) or both Dectus Medallion halves (by the lift).
- [ ] Leyndell's gate needs any two of: Godrick, Rennala, Radahn, Rykard, Mohg, Malenia.
      The scripts count the bosses killed, not the runes held. Worth confirming: kill two,
      hold no runes, and the gate opens.
- [ ] Mountaintops need the Rold Medallion. Consecrated Snowfield needs both Haligtree Secret Medallion halves.
- [ ] Mohgwyn Palace: the Pureblood Knight's Medal from anywhere, or the waygate in the Snowfield.
- [ ] Farum Azula after the Fire Giant, the Ashen Capital after Maliketh, the Elden Throne after Godfrey.

**Shadow of the Erdtree**
- [ ] The DLC (Mohg's arena, Miquella's arm) needs both Mohg and Radahn.
- [ ] Scadu Altus by Castle Ensis needs Rellana (the Ellac River way needs nothing; that was checked in play).
- [ ] Enir-Ilim needs Romina and Messmer's Kindling (burning the Sealing Tree).
- [ ] Scaduview needs Commander Gaius.
- [ ] Charo's Hidden Grave needs the Ancient Dragon-Man (over the stone dragon on Torrent).
      Guides disagree, so the map asks the most. Is there an earlier way in?
- [ ] Stone Coffin Fissure: the seal lifts once Miquella discards his rune at the Shadow
      Keep. The map stands in the Golden Hippopotamus for that. Where is the hole, on
      Charo's coast or the Cerulean Coast?
- [ ] Jagged Peak needs the Ancient Dragon-Man.

## 4. Assumptions with no source behind them

- [ ] Shop lists that no merchant's script opens are taken to be unused and never sold.
      If you ever see one of these on sale, tell me: plain starter weapons, armour, staves
      and dragon incantations; a Memory Stone and a Stonesword Key priced in Kalé's list.
- [ ] Places that shut for good: Leyndell and what's reached through it (the
      Shunning-Grounds, the West and East Altus towers, the Isolated Divine Tower) shut once
      the Erdtree burns. Volcano Manor and Farum Azula are taken **not** to shut.
- [ ] The Divine Towers' restored Great Runes are never counted on, as the map can't see
      their condition. Nothing to check unless you want them used.

## Already checked in play (2026-10-29)

Stormveil → Liurnia without Godrick; Leyndell → Forbidden Lands needs Morgott; the Lake of
Rot is open without Ranni's quest; Melina gives the Rold Medallion after Morgott; Albus gives
the right half of the Haligtree medallion; the Ellac River skips Castle Ensis; Varré gives the
Pureblood Knight's Medal; nobody in Leyndell gives the left half of the Haligtree medallion.
