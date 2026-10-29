//! Auto-upgrade (docs/spec/items.md §2.7): a weapon picked up or bought for
//! runes arrives at the highest level the player has raised a weapon of its
//! path to, never below its own level.
//!
//! A weapon id is its base id plus its level (Moonveil 9060000, Moonveil +5
//! 9060005). Its path is its EquipParamWeapon row's reinforce type, whose
//! ReinforceParamWeapon rows are that id plus each level: +25 for the regular
//! path, +10 for the somber one.
//!
//! **What counts as raised.** The game gives some weapons already upgraded
//! (25 map pick-ups give +1 to +10), so holding a weapon at a level proves
//! nothing on its own. A held weapon counts only above the highest level any
//! pick-up, shop or class in the loaded tables gives it at: that level can
//! only have come from a smith.

use std::collections::HashMap;

/// The two upgrade paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Path {
    /// Smithing Stones, to +25.
    Regular,
    /// Somber Smithing Stones, to +10.
    Somber,
}

/// The highest level raised on each path.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Levels {
    pub regular: i32,
    pub somber: i32,
}

impl Levels {
    pub fn of(&self, path: Path) -> i32 {
        match path {
            Path::Regular => self.regular,
            Path::Somber => self.somber,
        }
    }

    /// Each path's higher level.
    pub fn max(self, other: Levels) -> Levels {
        Levels {
            regular: self.regular.max(other.regular),
            somber: self.somber.max(other.somber),
        }
    }
}

/// The game's weapons and their paths.
#[derive(Debug, Default)]
pub struct Weapons {
    /// Reinforce type of every EquipParamWeapon row.
    reinforce: HashMap<i32, i32>,
    /// Last level of every reinforce type.
    last: HashMap<i32, i32>,
}

impl Weapons {
    /// From EquipParamWeapon's rows (id, reinforce type) and
    /// ReinforceParamWeapon's row ids.
    pub fn new(weapons: impl IntoIterator<Item = (i32, i32)>, reinforce: &[i32]) -> Self {
        let mut last = HashMap::new();
        for &id in reinforce {
            let e = last.entry(id - id % 100).or_insert(0);
            *e = (*e).max(id % 100);
        }
        Weapons {
            reinforce: weapons.into_iter().collect(),
            last,
        }
    }

    /// A weapon id's base id and level. A few rows' own ids do not end in
    /// 00, so an id that is a row is taken as level 0.
    pub fn split(&self, id: i32) -> Option<(i32, i32)> {
        if self.reinforce.contains_key(&id) {
            return Some((id, 0));
        }
        let base = id - id % 100;
        self.reinforce
            .contains_key(&base)
            .then_some((base, id % 100))
    }

    /// A base id's path, if it can be upgraded.
    pub fn path(&self, base: i32) -> Option<Path> {
        match self.last.get(self.reinforce.get(&base)?)? {
            25 => Some(Path::Regular),
            10 => Some(Path::Somber),
            _ => None,
        }
    }
}

/// One weapon slot of a pick-up or shop row: the address of its item id, and
/// the id the loaded tables put there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot {
    pub at: usize,
    pub own: i32,
}

/// Everything auto-upgrade needs, read once from the loaded tables.
#[derive(Debug)]
pub struct Upgrader {
    pub weapons: Weapons,
    /// The highest level each base id is given at anywhere.
    given: HashMap<i32, i32>,
    pub slots: Vec<Slot>,
}

impl Upgrader {
    /// `given`: every weapon id a pick-up, shop or class gives; `slots`: the
    /// pick-ups and shop rows to upgrade.
    pub fn new(weapons: Weapons, given: impl IntoIterator<Item = i32>, slots: Vec<Slot>) -> Self {
        let mut levels = HashMap::new();
        for id in given {
            if let Some((base, level)) = weapons.split(id) {
                let e = levels.entry(base).or_insert(0);
                *e = (*e).max(level);
            }
        }
        let slots = slots
            .into_iter()
            .filter(|s| {
                weapons
                    .split(s.own)
                    .is_some_and(|(b, _)| weapons.path(b).is_some())
            })
            .collect();
        Upgrader {
            weapons,
            given: levels,
            slots,
        }
    }

    /// The levels raised, from the weapon ids the player holds.
    pub fn raised(&self, held: impl IntoIterator<Item = i32>) -> Levels {
        let mut out = Levels::default();
        for id in held {
            let Some((base, level)) = self.weapons.split(id) else {
                continue;
            };
            if level <= self.given.get(&base).copied().unwrap_or(0) {
                continue;
            }
            match self.weapons.path(base) {
                Some(Path::Regular) => out.regular = out.regular.max(level),
                Some(Path::Somber) => out.somber = out.somber.max(level),
                None => {}
            }
        }
        out
    }

    /// The id a slot gives at these levels.
    pub fn target(&self, slot: Slot, levels: Levels) -> i32 {
        let Some((base, own)) = self.weapons.split(slot.own) else {
            return slot.own;
        };
        let Some(path) = self.weapons.path(base) else {
            return slot.own;
        };
        let cap = match path {
            Path::Regular => 25,
            Path::Somber => 10,
        };
        base + own.max(levels.of(path).min(cap))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAGGER: i32 = 1_000_000;
    const LONGSWORD: i32 = 2_000_000;
    const MOONVEIL: i32 = 9_060_000;
    const TORCH: i32 = 24_000_000;

    fn upgrader(slots: &[i32]) -> Upgrader {
        let reinforce: Vec<i32> = (0..=25).chain(2200..=2210).chain([3000]).collect();
        let weapons = Weapons::new(
            [(DAGGER, 0), (LONGSWORD, 0), (MOONVEIL, 2200), (TORCH, 3000)],
            &reinforce,
        );
        let slots = slots
            .iter()
            .enumerate()
            .map(|(i, &own)| Slot { at: i * 4, own })
            .collect();
        // A pick-up somewhere gives Longsword +7.
        Upgrader::new(weapons, [DAGGER, LONGSWORD + 7, MOONVEIL], slots)
    }

    #[test]
    fn levels_given_by_the_game_do_not_count_as_raised() {
        let u = upgrader(&[]);
        assert_eq!(u.raised([LONGSWORD + 7, MOONVEIL]), Levels::default());
        assert_eq!(
            u.raised([LONGSWORD + 8, DAGGER + 3, MOONVEIL + 4]),
            Levels {
                regular: 8,
                somber: 4
            }
        );
        // An unknown id and a weapon with no upgrades are ignored.
        assert_eq!(u.raised([123, TORCH + 5]), Levels::default());
    }

    #[test]
    fn pick_ups_arrive_at_the_level_of_their_own_path() {
        let u = upgrader(&[DAGGER, LONGSWORD + 7, MOONVEIL, TORCH]);
        // The torch cannot be upgraded, so it is not a slot.
        assert_eq!(u.slots.len(), 3);
        let at = |levels| -> Vec<i32> { u.slots.iter().map(|&s| u.target(s, levels)).collect() };
        assert_eq!(at(Levels::default()), [DAGGER, LONGSWORD + 7, MOONVEIL]);
        let l = Levels {
            regular: 5,
            somber: 3,
        };
        // Never below its own level: the Longsword stays +7.
        assert_eq!(at(l), [DAGGER + 5, LONGSWORD + 7, MOONVEIL + 3]);
        let l = Levels {
            regular: 25,
            somber: 10,
        };
        assert_eq!(at(l), [DAGGER + 25, LONGSWORD + 25, MOONVEIL + 10]);
    }
}
