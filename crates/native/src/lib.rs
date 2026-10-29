//! `apoc_native.dll`: the randomizer's own module inside the game, loaded by
//! me3 as a native alongside the mod folder. It is where options that depend
//! on what the player does during play live, starting with auto-upgrade
//! (docs/spec/items.md §2.7).
//!
//! Built for Windows (`cargo build -p native --target
//! x86_64-pc-windows-gnu`); on other targets it is an empty library, so the
//! workspace still builds and tests everywhere.
//!
//! On attach it appends a line to the file `APOC_NATIVE_LOG` names (a Windows
//! path), from its own thread, as file work under the loader lock is unsafe.
//!
//! **Probe** (`APOC_NATIVE_PROBE=1`, for working out the game's memory): every
//! 15 seconds it finds the game's item instances by their vtables and logs
//! the first words of each new one.
//!
//! Vtables are found at run time from the loaded game's own RTTI, by class
//! name, not fixed addresses: a game patch moves them (2.7.2 did), and on
//! disk the exe keeps its RTTI names packed.

/// The item-instance classes the probe looks for, with their namespace
/// (RTTI decorates `CS::CSWepGaitemIns` as `.?AVCSWepGaitemIns@CS@@`).
pub const ITEM_CLASSES: [&str; 3] = [
    "CSWepGaitemIns@CS",
    "CSProGaitemIns@CS",
    "CSGemGaitemIns@CS",
];

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

#[cfg(windows)]
mod windows {
    use std::collections::BTreeSet;
    use std::ffi::c_void;
    use std::io::Write;

    use super::*;

    const DLL_PROCESS_ATTACH: u32 = 1;
    const MEM_COMMIT: u32 = 0x1000;
    /// Page protections that can be written (and so read).
    const WRITABLE: u32 = 0x04 | 0x08 | 0x40 | 0x80;
    const GUARD: u32 = 0x100;
    const NO_ACCESS: u32 = 0x01;

    #[repr(C)]
    #[derive(Default)]
    struct MemoryBasicInformation {
        base_address: usize,
        allocation_base: usize,
        allocation_protect: u32,
        partition_id: u16,
        region_size: usize,
        state: u32,
        protect: u32,
        kind: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn VirtualQuery(address: usize, info: *mut MemoryBasicInformation, length: usize) -> usize;
        fn GetModuleHandleW(name: *const u16) -> usize;
    }

    fn note(line: &str) {
        let Ok(path) = std::env::var("APOC_NATIVE_LOG") else {
            return;
        };
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(f, "{line}");
        }
    }

    /// Every committed, writable region: where the game's objects live,
    /// private, mapped or in its own image. Logs totals by region type.
    fn heap_regions() -> Vec<(usize, usize)> {
        let mut by_kind = std::collections::BTreeMap::<u32, (usize, usize)>::new();
        let mut out = Vec::new();
        let mut at = 0x10000usize;
        loop {
            let mut info = MemoryBasicInformation::default();
            let size = std::mem::size_of::<MemoryBasicInformation>();
            if unsafe { VirtualQuery(at, &mut info, size) } == 0 {
                break;
            }
            if info.state == MEM_COMMIT && info.protect & WRITABLE != 0 && info.protect & GUARD == 0
            {
                out.push((info.base_address, info.region_size));
                let e = by_kind.entry(info.kind).or_default();
                e.0 += 1;
                e.1 += info.region_size;
            }
            let next = info.base_address.saturating_add(info.region_size);
            if next <= at || next >= 0x7FFF_FFFF_0000 {
                break;
            }
            at = next;
        }
        for (kind, (n, bytes)) in by_kind {
            note(&format!(
                "regions of type {kind:#x}: {n}, {} MB",
                bytes >> 20
            ));
        }
        out
    }

    /// Addresses of objects whose first word is `vtable`.
    fn instances(regions: &[(usize, usize)], vtable: usize) -> Vec<usize> {
        let mut out = Vec::new();
        for &(base, size) in regions {
            let words = unsafe { std::slice::from_raw_parts(base as *const usize, size / 8) };
            for (i, &w) in words.iter().enumerate() {
                if w == vtable {
                    out.push(base + i * 8);
                }
            }
        }
        out
    }

    /// A copy of the game's image as loaded, `SizeOfImage` bytes from its
    /// base, with pages that cannot be read left as zeros.
    fn loaded_image(module: usize) -> Vec<u8> {
        let pe = module + unsafe { *((module + 0x3c) as *const u32) } as usize;
        let size = unsafe { *((pe + 0x18 + 0x38) as *const u32) } as usize;
        let mut image = vec![0u8; size];
        let mut at = module;
        while at < module + size {
            let mut info = MemoryBasicInformation::default();
            let n = std::mem::size_of::<MemoryBasicInformation>();
            if unsafe { VirtualQuery(at, &mut info, n) } == 0 {
                break;
            }
            let end = (info.base_address + info.region_size).min(module + size);
            let readable = info.state == MEM_COMMIT && info.protect & (NO_ACCESS | GUARD) == 0;
            if readable {
                let from = unsafe { std::slice::from_raw_parts(at as *const u8, end - at) };
                image[at - module..end - module].copy_from_slice(from);
            }
            at = end;
        }
        image
    }

    /// Addresses of every 4-aligned `u32` equal to `value`.
    fn words_equal(regions: &[(usize, usize)], value: u32) -> Vec<usize> {
        let mut out = Vec::new();
        for &(base, size) in regions {
            let words = unsafe { std::slice::from_raw_parts(base as *const u32, size / 4) };
            for (i, &w) in words.iter().enumerate() {
                if w == value {
                    out.push(base + i * 4);
                }
            }
        }
        out
    }

    /// Moonveil, the boot-test save's weapon: where its id sits shows where
    /// the inventory keeps weapons.
    const MOONVEIL: u32 = 9_060_000;

    fn probe() {
        let module = unsafe { GetModuleHandleW(std::ptr::null()) };
        note(&format!("probe: game module at {module:#x}"));
        // Wait for the game to unpack itself before reading its RTTI.
        std::thread::sleep(std::time::Duration::from_secs(20));
        let image = loaded_image(module);
        let classes: Vec<(&str, usize)> = ITEM_CLASSES
            .iter()
            .filter_map(|&c| {
                let vt = rtti_vtable(&image, module, c);
                note(&format!("probe: {c} vtable {vt:x?}"));
                Some((c, vt?))
            })
            .collect();
        let mut seen = BTreeSet::new();
        for round in 0..=u32::MAX {
            std::thread::sleep(std::time::Duration::from_secs(15));
            let regions = heap_regions();
            let ids = words_equal(&regions, MOONVEIL);
            note(&format!(
                "probe {round}: {} copies of Moonveil's id",
                ids.len()
            ));
            for &a in ids.iter().filter(|a| seen.insert(**a)).take(40) {
                let words = unsafe { std::slice::from_raw_parts((a - 0x20) as *const u32, 16) };
                let hex: Vec<String> = words.iter().map(|w| format!("{w:08x}")).collect();
                note(&format!(
                    "probe {round}: id at {a:#x} (from -0x20): {}",
                    hex.join(" ")
                ));
            }
            for &(name, vt) in &classes {
                let found = instances(&regions, vt);
                let mut new = 0;
                for &a in &found {
                    if !seen.insert(a) {
                        continue;
                    }
                    new += 1;
                    let words = unsafe { std::slice::from_raw_parts(a as *const u32, 12) };
                    let hex: Vec<String> = words.iter().map(|w| format!("{w:08x}")).collect();
                    note(&format!(
                        "probe {round}: {name} at {a:#x}: {}",
                        hex.join(" ")
                    ));
                }
                note(&format!(
                    "probe {round}: {} {name} instances, {new} new",
                    found.len()
                ));
            }
        }
    }

    /// Called by Windows as the module is loaded and unloaded.
    #[unsafe(no_mangle)]
    #[allow(non_snake_case)]
    pub extern "system" fn DllMain(
        _module: *mut c_void,
        reason: u32,
        _reserved: *mut c_void,
    ) -> i32 {
        if reason == DLL_PROCESS_ATTACH {
            std::thread::spawn(|| {
                note(&format!(
                    "apoc_native {} loaded into process {}",
                    env!("CARGO_PKG_VERSION"),
                    std::process::id()
                ));
                if std::env::var("APOC_NATIVE_PROBE").is_ok() {
                    probe();
                }
            });
        }
        1
    }
}

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
