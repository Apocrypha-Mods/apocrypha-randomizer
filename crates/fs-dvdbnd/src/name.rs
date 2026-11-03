//! The hash an archive files a path under.
//!
//! The path is lowercased, back slashes become forward slashes, it gains a
//! leading `/` if it lacks one, and then `h = h * 0x85 + byte` over 64 bits.
//! An entry sits in bucket `hash % bucket_count`.
//!
//! Checked against the game's own tables: `/event/common.emevd.dcx`,
//! `/msg/engus/item.msgbnd.dcx` and `/map/mapstudio/m10_00_00_00.msb.dcx` each
//! hash to an entry, in the bucket that formula names.

/// The 64-bit hash of an archive path.
pub fn hash_path(path: &str) -> u64 {
    let prefix = (!path.starts_with(['/', '\\'])).then_some(b'/');
    prefix
        .into_iter()
        .chain(path.bytes())
        .map(|c| match c.to_ascii_lowercase() {
            b'\\' => b'/',
            c => c,
        })
        .fold(0u64, |h, c| h.wrapping_mul(0x85).wrapping_add(u64::from(c)))
}

fn hash_more(h: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(h, |h, &c| {
        let c = match c.to_ascii_lowercase() {
            b'\\' => b'/',
            c => c,
        };
        h.wrapping_mul(0x85).wrapping_add(u64::from(c))
    })
}

/// Every map id `mAA_BB_CC_DD`, each part 00 to 99, for which the path
/// `prefix + id + suffix` hashes to a value `has` accepts, in order.
///
/// All 10^8 ids are tried, the hash carried across the id's digits, so it
/// takes well under a second. A pattern that stops at some part (only
/// `DD` < 4, say) misses maps: the game's ids run to 10, 11, 12 and 99 in
/// the last part.
pub fn map_ids(prefix: &str, suffix: &str, has: impl Fn(u64) -> bool) -> Vec<String> {
    let start = hash_path(&format!("{prefix}m"));
    let tail = suffix.as_bytes();
    let scale = 0x85u64.wrapping_pow(tail.len() as u32);
    let tail_hash = hash_more(0, tail);
    let two = |h: u64, n: u32| hash_more(h, &[b'0' + (n / 10) as u8, b'0' + (n % 10) as u8]);
    let mut out = Vec::new();
    for a in 0..100 {
        let ha = hash_more(two(start, a), b"_");
        for b in 0..100 {
            let hb = hash_more(two(ha, b), b"_");
            for c in 0..100 {
                let hc = hash_more(two(hb, c), b"_");
                for d in 0..100 {
                    let h = two(hc, d).wrapping_mul(scale).wrapping_add(tail_hash);
                    if has(h) {
                        out.push(format!("m{a:02}_{b:02}_{c:02}_{d:02}"));
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_ids_are_swept_through_every_part() {
        let want = [
            hash_path("/map/mapstudio/m10_00_00_00.msb.dcx"),
            hash_path("/map/mapstudio/m60_44_36_10.msb.dcx"),
            hash_path("/map/mapstudio/m10_00_00_99.msb.dcx"),
            hash_path("/event/m60_44_36_10.emevd.dcx"),
        ];
        let ids = map_ids("/map/mapstudio/", ".msb.dcx", |h| want.contains(&h));
        assert_eq!(ids, ["m10_00_00_00", "m10_00_00_99", "m60_44_36_10"]);
    }

    #[test]
    fn case_slashes_and_the_leading_slash_do_not_change_the_hash() {
        let h = hash_path("/event/common.emevd.dcx");
        assert_eq!(h, hash_path("event/common.emevd.dcx"));
        assert_eq!(h, hash_path(r"\EVENT\Common.emevd.dcx"));
        assert_ne!(h, hash_path("/event/common.emevd"));
    }

    /// The value the game's own table files `/event/common.emevd.dcx` under, in
    /// `Data0` of build 25080141.
    #[test]
    fn a_known_path_hashes_to_the_value_the_game_files_it_under() {
        assert_eq!(hash_path("/event/common.emevd.dcx"), 0x5495_998c_192d_d5c9);
    }
}
