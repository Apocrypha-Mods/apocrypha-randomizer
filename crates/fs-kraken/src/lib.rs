//! Oodle Kraken decoding, the compression most of Elden Ring's files use.
//!
//! # Why this is its own crate
//!
//! The only pure-Rust decoder, `oozextract`, is labelled MIT but describes
//! itself as a port of `ooz`, which is GPL. Until that is settled by a legal
//! opinion (PLAN.md §6), nothing that ships may link it. So it lives here,
//! behind one function, and `fs-dcx` reaches it only through its `kraken`
//! feature, which is off by default. Swapping the backend (the game's own DLL
//! under Wine through a helper, or a clean-room decoder) changes this crate and
//! nothing else.
//!
//! Only decoding is offered. Files we write use zstd or deflate instead.

use fs_bytes::{Error, Result};

/// Decode a Kraken stream into exactly `size` bytes.
pub fn decompress(input: &[u8], size: usize) -> Result<Vec<u8>> {
    let mut out = vec![0u8; size];
    let n = oozextract::Extractor::new()
        .read_from_slice(input, &mut out)
        .map_err(|e| Error::Compression(format!("Kraken: {e:?}")))?;
    if n != size {
        return Err(Error::Compression(format!(
            "Kraken: the stream holds {n} bytes, not the {size} its header promises"
        )));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garbage_is_an_error_and_not_a_panic() {
        for input in [
            &[][..],
            &[0x8c],
            &[0x8c, 0x06, 0xff, 0xff, 0xff],
            &[0u8; 64],
        ] {
            let _ = decompress(input, 100);
        }
        assert!(decompress(&[0xde, 0xad], 4096).is_err());
    }
}
