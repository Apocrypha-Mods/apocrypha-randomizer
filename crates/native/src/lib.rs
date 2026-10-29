//! `apoc_native.dll`: the randomizer's own module inside the game, loaded by
//! me3 as a native alongside the mod folder. It is where options that depend
//! on what the player does during play live, starting with auto-upgrade
//! (docs/spec/items.md §2.7).
//!
//! Built for Windows (`cargo build -p native --target
//! x86_64-pc-windows-gnu`); on other targets the Windows glue is left out,
//! so the workspace still builds and tests everywhere.
//!
//! On attach it appends a line to the file `APOC_NATIVE_LOG` names (a Windows
//! path), from its own thread, as file work under the loader lock is unsafe.
//!
//! **Auto-upgrade**: it finds the live param tables ([`params`]) and the
//! player's inventory by following the game's own pointers ([`walk`]), and
//! every two seconds reads the player's weapons and writes the levels they
//! have raised into every weapon pick-up and rune-shop row ([`upgrade`]), so
//! a weapon is made at that level as it is got. Nothing in the game's code is
//! patched; only table values change. `APOC_NATIVE_LEVELS=7,3` sets a floor
//! under the regular and somber levels, for testing in the lab.
//!
//! Classes are found at run time from the loaded game's own RTTI, by name
//! (RTTI decorates `CS::PlayerGameData` as `.?AVPlayerGameData@CS@@`), not by
//! fixed addresses: a game patch moves them (2.7.2 did), and on disk the exe
//! keeps some RTTI names packed.

/// The vtable of `class` (its complete object, at offset 0) in a loaded
/// 64-bit image, from its RTTI: the type descriptor holding the decorated
/// name, the complete object locator naming that descriptor and itself by
/// image offset, and the vtable whose word before it points at the locator.
pub fn rtti_vtable(image: &[u8], base: usize, class: &str) -> Option<usize> {
    let u32_at = |o: usize| {
        image
            .get(o..o + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    };
    let name = format!(".?AV{class}@@\0");
    let name = name.as_bytes();
    for at in find_all(image, name) {
        let Some(td) = at.checked_sub(0x10) else {
            continue;
        };
        let td = (td as u32).to_le_bytes();
        for f in find_all(image, &td) {
            let Some(col) = f.checked_sub(0xc) else {
                continue;
            };
            let is_col = col % 4 == 0
                && u32_at(col) == Some(1)
                && u32_at(col + 4) == Some(0)
                && u32_at(col + 0x14) == Some(col as u32);
            if !is_col {
                continue;
            }
            let ptr = ((base + col) as u64).to_le_bytes();
            if let Some(v) = find_all(image, &ptr).into_iter().find(|v| v % 8 == 0) {
                return Some(base + v + 8);
            }
        }
    }
    None
}

fn find_all(hay: &[u8], needle: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    let Some(&first) = needle.first() else {
        return out;
    };
    let mut i = 0;
    while let Some(p) = hay[i..].iter().position(|&b| b == first) {
        let at = i + p;
        if hay[at..].starts_with(needle) {
            out.push(at);
        }
        i = at + 1;
    }
    out
}

pub mod params;
pub mod upgrade;
pub mod walk;

#[cfg(windows)]
mod windows;

#[cfg(test)]
mod tests {
    use super::*;

    /// A made-up image holding one class's RTTI chain: its name, a type
    /// descriptor 0x10 before it, a locator naming both by image offset,
    /// and a vtable whose word before it points at the locator.
    #[test]
    fn vtables_are_found_from_rtti() {
        let base = 0x1_4000_0000usize;
        let mut image = vec![0u8; 0x400];
        let put =
            |image: &mut Vec<u8>, at: usize, b: &[u8]| image[at..at + b.len()].copy_from_slice(b);
        put(&mut image, 0x110, b".?AVCSWepGaitemIns@CS@@\0");
        let col = 0x200;
        put(&mut image, col, &1u32.to_le_bytes());
        put(&mut image, col + 0xc, &0x100u32.to_le_bytes());
        put(&mut image, col + 0x14, &(col as u32).to_le_bytes());
        put(&mut image, 0x300, &((base + col) as u64).to_le_bytes());
        assert_eq!(
            rtti_vtable(&image, base, "CSWepGaitemIns@CS"),
            Some(base + 0x308)
        );
        assert_eq!(rtti_vtable(&image, base, "CSWepGaitemIns"), None);
    }
}
