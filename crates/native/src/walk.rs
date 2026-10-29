//! Finding the game's objects by following its own pointers, so the module
//! never has to read all of the game's memory: reading pages the game
//! reserved but never touched makes the system back them, and in the lab a
//! scan of everything pushed the game past its 9 GB cap.
//!
//! Every object found this way was worked out in the lab on 2.7.2
//! (docs/progress.md, "The native module"):
//!
//! - the game's image holds a pointer to the param store
//!   (`CS::ParamRepositoryImp`), from which a few pointers lead to the live
//!   copy of each param table;
//! - it holds a pointer to a `CS::PlayerGameData`, from which two pointers
//!   lead to the player's `CS::EquipInventoryData`.

use std::collections::{BTreeSet, VecDeque};

/// How much of a root object is read, and of every object after it.
pub const ROOT_LEN: usize = 0x4000;
pub const NODE_LEN: usize = 0x400;

/// Walk pointers breadth first from `roots`, up to `depth` steps and `limit`
/// objects, calling `visit(address, bytes, depth)` on each object read.
/// `read` reads memory, and `pointer` says whether a word may be followed.
pub fn walk(
    roots: &[usize],
    depth: u32,
    limit: usize,
    read: impl Fn(usize, usize) -> Option<Vec<u8>>,
    pointer: impl Fn(usize) -> bool,
    mut visit: impl FnMut(usize, &[u8], u32),
) {
    let mut seen = BTreeSet::new();
    let mut queue: VecDeque<(usize, u32)> = roots.iter().map(|&r| (r, 0)).collect();
    while let Some((at, d)) = queue.pop_front() {
        if seen.len() >= limit || !seen.insert(at) {
            continue;
        }
        let len = if d == 0 { ROOT_LEN } else { NODE_LEN };
        let Some(b) = read(at, len) else {
            continue;
        };
        visit(at, &b, d);
        if d < depth {
            for o in (0..b.len().saturating_sub(7)).step_by(8) {
                let w = usize::from_le_bytes(b[o..o + 8].try_into().unwrap());
                if w % 8 == 0 && pointer(w) {
                    queue.push_back((w, d + 1));
                }
            }
        }
    }
}

/// Offsets in `b` whose word is `vtable`: this object, if it is one, and
/// objects embedded in it.
pub fn embedded(b: &[u8], vtable: usize) -> impl Iterator<Item = usize> + '_ {
    (0..b.len().saturating_sub(7))
        .step_by(8)
        .filter(move |&o| usize::from_le_bytes(b[o..o + 8].try_into().unwrap()) == vtable)
}

/// An `EquipInventoryData`: its record count (capacity) at +0x08 and the
/// address of its records at +0x10.
pub fn inventory_list(b: &[u8]) -> Option<(usize, usize)> {
    let cap = u32::from_le_bytes(b.get(8..12)?.try_into().ok()?) as usize;
    let at = usize::from_le_bytes(b.get(0x10..0x18)?.try_into().ok()?);
    Some((cap, at))
}

/// The length of one inventory record: handle, item id, count, list index
/// and two words not needed here.
pub const RECORD_LEN: usize = 0x18;

/// The item ids of the weapons among inventory records: those whose handle
/// starts with 8 (a weapon instance's handle, `0x8080…`).
pub fn weapons(records: &[u8]) -> Vec<i32> {
    records
        .chunks_exact(RECORD_LEN)
        .filter_map(|r| {
            let handle = u32::from_le_bytes(r[0..4].try_into().unwrap());
            let id = i32::from_le_bytes(r[4..8].try_into().unwrap());
            (handle >> 28 == 8 && id > 0).then_some(id)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Made-up memory: objects at fixed addresses.
    struct Mem(BTreeMap<usize, Vec<u8>>);

    impl Mem {
        fn read(&self, at: usize, len: usize) -> Option<Vec<u8>> {
            let (&base, b) = self.0.range(..=at).next_back()?;
            let from = at - base;
            let mut out = b.get(from..)?.to_vec();
            out.resize(len, 0);
            Some(out)
        }
    }

    fn words(ws: &[u64]) -> Vec<u8> {
        ws.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    #[test]
    fn objects_are_found_two_steps_away_and_not_further() {
        const VT: u64 = 0x1_4000_0100;
        let mut m = BTreeMap::new();
        m.insert(0x1000, words(&[0xAAAA, 0x2000]));
        m.insert(0x2000, words(&[0, 0, 0x3000]));
        // An object embedding the one looked for at +0x10, and a deeper one.
        m.insert(0x3000, words(&[0xBBBB, 0x4000, VT, 7]));
        m.insert(0x4000, words(&[VT]));
        let mem = Mem(m);
        let mut found = Vec::new();
        walk(
            &[0x1000],
            2,
            100,
            |a, l| mem.read(a, l),
            |w| (0x1000..0x5000).contains(&w),
            |at, b, _| found.extend(embedded(b, VT as usize).map(|o| at + o)),
        );
        assert_eq!(found, [0x3010]);
    }

    #[test]
    fn weapons_are_read_from_inventory_records() {
        // Records as the lab found them: Heater Shield, Moonveil, a
        // protector, an empty slot.
        let rec = |h: u32, id: u32| -> Vec<u8> {
            [h, id, 1, 0x7b, 1, 0xffff_ffff]
                .iter()
                .flat_map(|w| w.to_le_bytes())
                .collect()
        };
        let b: Vec<u8> = [
            rec(0x8080_007b, 31_330_000),
            rec(0x8080_007c, 9_060_000),
            rec(0x9080_0081, 0x100a_1220),
            rec(0, 0xffff_ffff),
        ]
        .concat();
        assert_eq!(weapons(&b), [31_330_000, 9_060_000]);
        let inv = words(&[0x1_429e_7f28, 0x0a8_0000_0c00, 0x7fff_9fd1_8d00]);
        assert_eq!(inventory_list(&inv), Some((0xc00, 0x7fff_9fd1_8d00)));
    }
}
