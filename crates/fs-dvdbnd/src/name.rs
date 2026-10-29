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

#[cfg(test)]
mod tests {
    use super::*;

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
