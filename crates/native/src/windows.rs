use std::ffi::c_void;
use std::io::Write;

use super::*;
use crate::params::{self, Table};
use crate::upgrade::{Levels, Slot, Upgrader, Weapons};
use crate::walk;

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
    fn GetCurrentProcess() -> usize;
    fn GetCurrentThread() -> usize;
    fn SetThreadPriority(thread: usize, priority: i32) -> i32;
    fn ReadProcessMemory(
        process: usize,
        address: usize,
        buffer: *mut u8,
        size: usize,
        read: *mut usize,
    ) -> i32;
    fn WriteProcessMemory(
        process: usize,
        address: usize,
        buffer: *const u8,
        size: usize,
        written: *mut usize,
    ) -> i32;
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

/// Every committed, writable region, as (base, size).
fn regions() -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut at = 0x10000usize;
    loop {
        let mut info = MemoryBasicInformation::default();
        let size = std::mem::size_of::<MemoryBasicInformation>();
        if unsafe { VirtualQuery(at, &mut info, size) } == 0 {
            break;
        }
        if info.state == MEM_COMMIT && info.protect & WRITABLE != 0 && info.protect & GUARD == 0 {
            out.push((info.base_address, info.region_size));
        }
        let next = info.base_address.saturating_add(info.region_size);
        if next <= at || next >= 0x7FFF_FFFF_0000 {
            break;
        }
        at = next;
    }
    out
}

/// `len` bytes from `at`, or `None` if any of them cannot be read. Reading
/// through the process handle rather than a pointer means memory the game
/// frees meanwhile fails the read instead of crashing the game.
fn read(at: usize, len: usize) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; len];
    let mut n = 0;
    let ok = unsafe { ReadProcessMemory(GetCurrentProcess(), at, buf.as_mut_ptr(), len, &mut n) };
    (ok != 0 && n == len).then_some(buf)
}

fn write_i32(at: usize, value: i32) -> bool {
    let b = value.to_le_bytes();
    let mut n = 0;
    unsafe { WriteProcessMemory(GetCurrentProcess(), at, b.as_ptr(), 4, &mut n) != 0 && n == 4 }
}

fn read_i32(at: usize) -> Option<i32> {
    Some(i32::from_le_bytes(read(at, 4)?.try_into().ok()?))
}

/// Whether `at` lies in one of `regions` (sorted by base).
fn inside(regions: &[(usize, usize)], at: usize) -> bool {
    let i = regions.partition_point(|&(b, _)| b <= at);
    i > 0 && at < regions[i - 1].0 + regions[i - 1].1
}

/// The game's objects of a class that its image points at: the image's
/// words that are the address of an object whose first word is `vtable`.
fn roots(image: &[u8], module: usize, regions: &[(usize, usize)], vtable: usize) -> Vec<usize> {
    let mut out = Vec::new();
    for o in (0..image.len().saturating_sub(7)).step_by(8) {
        let v = usize::from_le_bytes(image[o..o + 8].try_into().unwrap());
        if v % 8 != 0 || (module..module + image.len()).contains(&v) || !inside(regions, v) {
            continue;
        }
        if read(v, 8).is_some_and(|b| usize::from_le_bytes(b.try_into().unwrap()) == vtable) {
            out.push(v);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The tables auto-upgrade reads, by type name.
const TABLES: [&str; 5] = [
    "ITEMLOT_PARAM_ST",
    "SHOP_LINEUP_PARAM",
    "CHARACTER_INIT_PARAM",
    "EQUIP_PARAM_WEAPON_ST",
    "REINFORCE_PARAM_WEAPON_ST",
];

/// The live tables of the types in [`TABLES`]: those the param store leads
/// to, five pointers deep in the 2.7.2 lab. The game also keeps two other
/// copies of the regulation, which it does not give items from.
fn live_tables(store: &[usize], regions: &[(usize, usize)]) -> Vec<(&'static str, Table)> {
    let mut heads = Vec::new();
    walk::walk(
        store,
        6,
        400_000,
        read,
        |w| inside(regions, w),
        |at, b, _| {
            if let Some((_, type_at)) = params::header(b) {
                heads.push((at, type_at));
            }
        },
    );
    let mut out = Vec::new();
    for (at, type_at) in heads {
        for name in TABLES {
            let Some(bytes) = read(at, type_at + name.len() + 1) else {
                continue;
            };
            if let Some(t) = params::table(&bytes, at, name) {
                out.push((name, t));
            }
        }
    }
    out.sort_by_key(|(_, t)| t.at);
    out.dedup_by_key(|(_, t)| t.at);
    out
}

/// Every value of a repeated i32 field in every row of the tables.
fn values<'a>(
    tables: impl IntoIterator<Item = &'a Table>,
    offset: usize,
    count: usize,
) -> Vec<i32> {
    let mut out = Vec::new();
    for t in tables {
        for &(_, row) in &t.rows {
            for i in 0..count {
                out.extend(read_i32(row + offset + i * 4));
            }
        }
    }
    out
}

/// Read the loaded tables into an [`Upgrader`].
fn upgrader(found: &[(&'static str, Table)]) -> Option<Upgrader> {
    let of = |name: &str| -> Vec<&Table> {
        found
            .iter()
            .filter(|(n, _)| *n == name)
            .map(|(_, t)| t)
            .collect()
    };
    for name in TABLES {
        let t = of(name);
        note(&format!(
            "upgrade: {} {name} table(s): {}",
            t.len(),
            t.iter()
                .map(|t| format!("{} rows at {:#x}", t.rows.len(), t.at))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let (lots, shops, classes) = (
        of("ITEMLOT_PARAM_ST"),
        of("SHOP_LINEUP_PARAM"),
        of("CHARACTER_INIT_PARAM"),
    );
    let weapon_rows = of("EQUIP_PARAM_WEAPON_ST");
    let reinforce = of("REINFORCE_PARAM_WEAPON_ST");
    if lots.is_empty() || weapon_rows.len() != 1 || reinforce.len() != 1 {
        return None;
    }
    // EquipParamWeapon's reinforce type: a u16 at 0xDA.
    let weapons = Weapons::new(
        weapon_rows[0].rows.iter().filter_map(|&(id, row)| {
            let b = read(row + 0xDA, 2)?;
            Some((id, i32::from(u16::from_le_bytes([b[0], b[1]]))))
        }),
        &reinforce[0]
            .rows
            .iter()
            .map(|&(id, _)| id)
            .collect::<Vec<_>>(),
    );
    // ItemLotParam: eight item ids from 0x00, their categories from 0x20
    // (2 is a weapon).
    let mut slots = Vec::new();
    for t in &lots {
        for &(_, row) in &t.rows {
            let Some(b) = read(row, 0x40) else {
                continue;
            };
            let word = |o: usize| i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
            for i in 0..8 {
                let (own, cat) = (word(i * 4), word(0x20 + i * 4));
                if cat == 2 && own > 0 {
                    slots.push(Slot {
                        at: row + i * 4,
                        own,
                    });
                }
            }
        }
    }
    // ShopLineupParam: equip id at 0x00, material set at 0x08, equip type
    // at 0x17 (0 is a weapon) and currency at 0x18 (0 is runes). A weapon
    // sold for runes alone is upgraded like a pick-up; one paid for in
    // items (a remembrance trade, Dragon Communion) is left as it is.
    let mut given: Vec<i32> = slots.iter().map(|s| s.own).collect();
    for t in &shops {
        for &(_, row) in &t.rows {
            let Some(b) = read(row, 0x19) else {
                continue;
            };
            let word = |o: usize| i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
            if b[0x17] != 0 || word(0) <= 0 {
                continue;
            }
            given.push(word(0));
            if b[0x18] == 0 && word(0x08) <= 0 {
                slots.push(Slot {
                    at: row,
                    own: word(0),
                });
            }
        }
    }
    // CharaInitParam: weapons at 0x10 (four) and 0x108 (two).
    given.extend(values(classes.iter().copied(), 0x10, 4));
    given.extend(values(classes.iter().copied(), 0x108, 2));
    let u = Upgrader::new(weapons, given, slots);
    note(&format!(
        "upgrade: {} weapon pick-ups and shop rows to upgrade",
        u.slots.len()
    ));
    Some(u)
}

/// A floor under the levels, for testing in the lab:
/// `APOC_NATIVE_LEVELS=regular,somber`.
fn floor() -> Levels {
    let Ok(v) = std::env::var("APOC_NATIVE_LEVELS") else {
        return Levels::default();
    };
    let mut it = v.split(',').map(|n| n.trim().parse().unwrap_or(0));
    Levels {
        regular: it.next().unwrap_or(0),
        somber: it.next().unwrap_or(0),
    }
}

/// The player's inventories: every `EquipInventoryData` within two pointers
/// of the player data the image points at. Offline that is the player's own.
fn inventories(player: &[usize], vtable: usize, regions: &[(usize, usize)]) -> Vec<usize> {
    let mut out = Vec::new();
    walk::walk(
        player,
        2,
        100_000,
        read,
        |w| inside(regions, w),
        |at, b, _| out.extend(walk::embedded(b, vtable).map(|o| at + o)),
    );
    out.sort();
    out.dedup();
    out
}

/// The ids of the weapons in the inventories.
fn held(inventories: &[usize]) -> Vec<i32> {
    let mut out = Vec::new();
    for &inv in inventories {
        let Some((cap, at)) = read(inv, 0x18).and_then(|b| walk::inventory_list(&b)) else {
            continue;
        };
        if let Some(records) = read(at, cap.min(0x2000) * walk::RECORD_LEN) {
            out.extend(walk::weapons(&records));
        }
    }
    out
}

/// Auto-upgrade: every two seconds, read the levels the player has raised
/// and write them into the weapon pick-ups, so the next one is made at that
/// level.
fn auto_upgrade() {
    // THREAD_PRIORITY_LOWEST: the game's own threads come first.
    unsafe { SetThreadPriority(GetCurrentThread(), -2) };
    let module = unsafe { GetModuleHandleW(std::ptr::null()) };
    let vtable = |image: &[u8], class| rtti_vtable(image, module, class);
    let (u, player, inventory) = loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
        let image = loaded_image(module);
        let (Some(store), Some(player), Some(inventory)) = (
            vtable(&image, "ParamRepositoryImp@CS"),
            vtable(&image, "PlayerGameData@CS"),
            vtable(&image, "EquipInventoryData@CS"),
        ) else {
            continue;
        };
        let regions = regions();
        let started = std::time::Instant::now();
        let tables = live_tables(&roots(&image, module, &regions, store), &regions);
        let Some(u) = upgrader(&tables) else {
            continue;
        };
        note(&format!(
            "upgrade: tables read in {} ms",
            started.elapsed().as_millis()
        ));
        break (u, roots(&image, module, &regions, player), inventory);
    };
    let floor = floor();
    let mut invs = Vec::new();
    let mut current = None;
    let mut last_held = Vec::new();
    for tick in 0u64..=u64::MAX {
        // Look for the inventories again every half minute, as loading a
        // character moves them.
        let started = std::time::Instant::now();
        if tick % 15 == 0 || invs.is_empty() {
            invs = inventories(&player, inventory, &regions());
        }
        let mut ids = held(&invs);
        ids.sort();
        if ids != last_held {
            note(&format!("upgrade: weapons held: {ids:?}"));
            last_held = ids.clone();
        }
        let levels = u.raised(ids.iter().copied()).max(floor);
        if current != Some(levels) {
            let mut written = 0;
            for &s in &u.slots {
                if write_i32(s.at, u.target(s, levels)) {
                    written += 1;
                }
            }
            note(&format!(
                "upgrade: {} weapons held in {} inventories, raised to +{} regular and +{} somber; {written} pick-ups rewritten ({} ms)",
                ids.len(),
                invs.len(),
                levels.regular,
                levels.somber,
                started.elapsed().as_millis()
            ));
            current = Some(levels);
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}

/// Called by Windows as the module is loaded and unloaded.
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "system" fn DllMain(_module: *mut c_void, reason: u32, _reserved: *mut c_void) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
        std::thread::spawn(|| {
            note(&format!(
                "apoc_native {} loaded into process {}",
                env!("CARGO_PKG_VERSION"),
                std::process::id()
            ));
            auto_upgrade();
        });
    }
    1
}
