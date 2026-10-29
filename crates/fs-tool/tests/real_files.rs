//! Tests against the player's own game files.
//!
//! They read the stock copy named by `APOC_ER_STOCK` (PLAN.md §5, Phase 0) and
//! skip, passing, when it is unset: game files are never in the repository.
//! The whole-game gate is `apoc-fs check`; these are the quick ones.

use std::path::PathBuf;
use std::sync::OnceLock;

use fs_bnd4::Bnd4;
use fs_dvdbnd::Dvd;

fn dvd() -> Option<&'static Dvd> {
    static DVD: OnceLock<Option<Dvd>> = OnceLock::new();
    DVD.get_or_init(|| {
        let dir = PathBuf::from(std::env::var_os("APOC_ER_STOCK")?);
        Some(Dvd::open_elden_ring(dir).expect("the stock copy opens"))
    })
    .as_ref()
}

macro_rules! game_or_skip {
    () => {
        match dvd() {
            Some(d) => d,
            None => {
                eprintln!("APOC_ER_STOCK is not set; skipping");
                return;
            }
        }
    };
}

fn read_plain(dvd: &Dvd, path: &str) -> Vec<u8> {
    let stored = dvd
        .read_path(path)
        .unwrap_or_else(|e| panic!("{path}: {e}"));
    fs_dcx::decompress(&stored)
        .unwrap_or_else(|e| panic!("{path}: {e}"))
        .1
}

#[test]
fn every_archive_opens_with_a_key_from_the_exe() {
    let dvd = game_or_skip!();
    let names: Vec<&str> = dvd.archives.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, fs_dvdbnd::ELDEN_RING_ARCHIVES);
    for a in &dvd.archives {
        assert!(!a.bhd.entries.is_empty(), "{} is empty", a.name);
        let end = a
            .bhd
            .entries
            .iter()
            .map(|e| e.offset + u64::from(e.padded_size))
            .max()
            .unwrap();
        assert_eq!(
            end,
            a.bdt_len(),
            "{}: the last entry ends where the .bdt does",
            a.name
        );
    }
}

#[test]
fn a_kraken_event_script_decompresses() {
    let dvd = game_or_skip!();
    let stored = dvd.read_path("/event/common.emevd.dcx").unwrap();
    let (header, plain) = fs_dcx::decompress(&stored).unwrap();
    assert_eq!(header.algorithm, fs_dcx::Algorithm::Kraken);
    assert!(plain.starts_with(b"EVD\0"), "{:?}", &plain[..4]);
}

#[test]
fn text_and_character_binders_round_trip_byte_for_byte() {
    let dvd = game_or_skip!();
    for path in ["/msg/engus/item.msgbnd.dcx", "/chr/c2010.chrbnd.dcx"] {
        let plain = read_plain(dvd, path);
        let bnd = Bnd4::read(&plain).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert!(!bnd.files.is_empty());
        assert!(
            bnd.write().unwrap() == plain,
            "{path} differs when rewritten"
        );
    }
}

#[test]
fn a_map_rewritten_as_zstd_reads_back_the_same() {
    let dvd = game_or_skip!();
    let stored = dvd
        .read_path("/map/mapstudio/m10_00_00_00.msb.dcx")
        .unwrap();
    let (header, plain) = fs_dcx::decompress(&stored).unwrap();
    assert!(plain.starts_with(b"MSB "));
    let zstd = fs_dcx::compress(&header.with_algorithm(fs_dcx::Algorithm::Zstd), &plain).unwrap();
    assert_eq!(fs_dcx::decompress(&zstd).unwrap().1, plain);
}
