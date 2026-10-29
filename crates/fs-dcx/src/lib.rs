//! The DCX container: a 0x4C-byte header, then one compressed stream.
//!
//! Lifted from `apoc-fsparams` (`dcx.rs`, Nova's own code) and widened to the
//! three compressions Elden Ring's archives use. The header, big-endian:
//!
//! ```text
//! 0x00 "DCX\0"  0x04 version  0x08 0x18  0x0C 0x24  0x10 0x44  0x14 0x4C
//! 0x18 "DCS\0"  0x1C uncompressed size   0x20 compressed size
//! 0x24 "DCP\0"  0x28 "KRAK" | "ZSTD" | "DFLT"   0x2C 0x20   0x30 level
//! 0x44 "DCA\0"  0x48 8                   0x4C data
//! ```
//!
//! Seen in Elden Ring 1.17.2's archives: Kraken, version 0x11000, level 6 (103,286
//! files); deflate, version 0x10000, level 9 (20 files, 19 of them storing 0x24
//! and 0x2C at 0x10 and 0x14). No zstd; zstd is what `regulation.bin` uses.
//!
//! Kraken is decoded only with the `kraken` feature (see `fs-kraken` on why),
//! and never written: a file we write uses zstd or deflate, under the header of
//! the file it replaces with only the compression and sizes changed.

use std::io::{Read, Write};

use fs_bytes::{self as rd, Error, Result, format_err};

/// Length of the header, and so the offset of the compressed data.
pub const HEADER_LEN: usize = 0x4C;

/// The largest payload decompressed by default. The game's largest file is
/// well under this; a header claiming more is broken or hostile, and the claim
/// would otherwise be allocated before anything else is checked.
pub const MAX_UNCOMPRESSED: usize = 1024 * 1024 * 1024;

/// The compression a container uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Algorithm {
    Kraken,
    Zstd,
    Deflate,
}

impl Algorithm {
    pub fn tag(self) -> &'static [u8; 4] {
        match self {
            Algorithm::Kraken => b"KRAK",
            Algorithm::Zstd => b"ZSTD",
            Algorithm::Deflate => b"DFLT",
        }
    }

    fn from_tag(tag: &[u8]) -> Result<Self> {
        match tag {
            b"KRAK" => Ok(Algorithm::Kraken),
            b"ZSTD" => Ok(Algorithm::Zstd),
            b"DFLT" => Ok(Algorithm::Deflate),
            other => Err(Error::Unsupported(format!(
                "this file is compressed with {}, which is not supported",
                String::from_utf8_lossy(other)
            ))),
        }
    }
}

/// A DCX header, kept whole so a rewritten file carries the original's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    raw: [u8; HEADER_LEN],
    pub algorithm: Algorithm,
}

impl Header {
    pub fn read(b: &[u8]) -> Result<Self> {
        for (at, magic) in [
            (0x00, b"DCX\0"),
            (0x18, b"DCS\0"),
            (0x24, b"DCP\0"),
            (0x44, b"DCA\0"),
        ] {
            if rd::bytes(b, at, 4, "DCX header")? != magic {
                return Err(format_err(format!(
                    "not a DCX container: expected {} at {at:#x}",
                    String::from_utf8_lossy(magic)
                )));
            }
        }
        // The values the header stores at 0x08–0x14. The sections sit at the
        // same places in every file, but two sets of values are stored: most
        // files have 0x44 and 0x4C, and 19 deflate files in Elden Ring's Data0
        // have 0x24 and 0x2C. Any other set is another DCX version.
        let mut stored = [0u32; 4];
        for (v, at) in stored.iter_mut().zip([0x08, 0x0C, 0x10, 0x14]) {
            *v = rd::u32_be(b, at, "DCX header")?;
        }
        if stored != [0x18, 0x24, 0x44, 0x4C] && stored != [0x18, 0x24, 0x24, 0x2C] {
            return Err(Error::Unsupported(
                "this DCX header layout is not supported".into(),
            ));
        }
        let algorithm = Algorithm::from_tag(rd::bytes(b, 0x28, 4, "DCX header")?)?;
        let mut raw = [0u8; HEADER_LEN];
        raw.copy_from_slice(rd::bytes(b, 0, HEADER_LEN, "DCX header")?);
        Ok(Header { raw, algorithm })
    }

    /// A header for a new file, for when there is no original to copy.
    pub fn new(algorithm: Algorithm) -> Self {
        let mut raw = [0u8; HEADER_LEN];
        let mut put = |at: usize, v: &[u8]| raw[at..at + v.len()].copy_from_slice(v);
        put(0x00, b"DCX\0");
        put(0x04, &0x0001_1000u32.to_be_bytes());
        put(0x08, &0x18u32.to_be_bytes());
        put(0x0C, &0x24u32.to_be_bytes());
        put(0x10, &0x44u32.to_be_bytes());
        put(0x14, &0x4Cu32.to_be_bytes());
        put(0x18, b"DCS\0");
        put(0x24, b"DCP\0");
        put(0x28, algorithm.tag());
        put(0x2C, &0x20u32.to_be_bytes());
        put(0x40, &0x0001_0100u32.to_be_bytes());
        put(0x44, b"DCA\0");
        put(0x48, &8u32.to_be_bytes());
        Header { raw, algorithm }
    }

    /// The same header with another compression, for rewriting a Kraken file
    /// in a compression we can write.
    pub fn with_algorithm(&self, algorithm: Algorithm) -> Self {
        let mut raw = self.raw;
        raw[0x28..0x2C].copy_from_slice(algorithm.tag());
        Header { raw, algorithm }
    }

    pub fn uncompressed_size(&self) -> usize {
        u32::from_be_bytes(self.raw[0x1C..0x20].try_into().unwrap()) as usize
    }

    pub fn compressed_size(&self) -> usize {
        u32::from_be_bytes(self.raw[0x20..0x24].try_into().unwrap()) as usize
    }

    /// The level byte, as the game's own files record it.
    pub fn level(&self) -> u8 {
        self.raw[0x30]
    }

    pub fn raw(&self) -> &[u8; HEADER_LEN] {
        &self.raw
    }
}

/// Whether `b` starts like a DCX container.
pub fn is_dcx(b: &[u8]) -> bool {
    b.starts_with(b"DCX\0")
}

/// Read the header and decompress the body.
pub fn decompress(b: &[u8]) -> Result<(Header, Vec<u8>)> {
    decompress_limited(b, MAX_UNCOMPRESSED)
}

/// [`decompress`], refusing a payload larger than `max`.
pub fn decompress_limited(b: &[u8], max: usize) -> Result<(Header, Vec<u8>)> {
    let header = Header::read(b)?;
    let size = header.uncompressed_size();
    let body = rd::bytes(b, HEADER_LEN, header.compressed_size(), "compressed data")?;
    if size > max {
        return Err(format_err(format!(
            "the file claims to hold {size} bytes, more than the {max} allowed"
        )));
    }
    let data = match header.algorithm {
        Algorithm::Kraken => kraken(body, size)?,
        Algorithm::Zstd => {
            zstd::bulk::decompress(body, size).map_err(|e| Error::Compression(e.to_string()))?
        }
        Algorithm::Deflate => {
            // Bounded by the declared size, plus one byte to notice a stream
            // that runs past it, so a zlib bomb stops at the claim.
            let mut out = Vec::with_capacity(size);
            flate2::read::ZlibDecoder::new(body)
                .take(size as u64 + 1)
                .read_to_end(&mut out)
                .map_err(|e| Error::Compression(e.to_string()))?;
            out
        }
    };
    if data.len() != size {
        return Err(Error::Compression(format!(
            "the header promises {size} bytes and the data holds {}",
            data.len()
        )));
    }
    Ok((header, data))
}

#[cfg(feature = "kraken")]
fn kraken(body: &[u8], size: usize) -> Result<Vec<u8>> {
    fs_kraken::decompress(body, size)
}

#[cfg(not(feature = "kraken"))]
fn kraken(_: &[u8], _: usize) -> Result<Vec<u8>> {
    Err(Error::Unsupported(
        "this file is Kraken-compressed, and this build cannot decode Kraken".into(),
    ))
}

/// The zstd level a rewritten file is compressed at.
///
/// Measured by `apoc-fsparams` on the game's 54 MB `regulation.bin`: level 9 is
/// within 2% of level 15's size and several times faster.
pub const ZSTD_LEVEL: i32 = 9;

/// The zstd window a rewritten file is compressed with, as a power of two.
///
/// 64 KiB, not the library's default. Found by `apoc-fsparams` by launching the
/// game: `regulation.bin` recompressed with zstd's default window for its size
/// (4 MiB) made Elden Ring raise an exception about ten seconds in, while the
/// same bytes with a 64 KiB window ran. Whether other files care is not yet
/// known; the safe choice is the same window everywhere.
pub const ZSTD_WINDOW_LOG: u32 = 16;

/// Compress `data` under a copy of `template`, with the sizes updated.
///
/// For zstd the frame has a 64 KiB window and no content size (the size lives
/// in the DCX header), and the level byte records the level used. The result
/// is padded with zeros to 16 bytes, as the game's files are.
pub fn compress(template: &Header, data: &[u8]) -> Result<Vec<u8>> {
    let body = match template.algorithm {
        Algorithm::Kraken => {
            return Err(Error::Unsupported(
                "Kraken cannot be written; rewrite the file as zstd or deflate".into(),
            ));
        }
        Algorithm::Zstd => {
            use zstd::zstd_safe::CParameter;
            let compression = |e: std::io::Error| Error::Compression(e.to_string());
            let mut c = zstd::bulk::Compressor::new(ZSTD_LEVEL).map_err(compression)?;
            c.set_parameter(CParameter::WindowLog(ZSTD_WINDOW_LOG))
                .map_err(compression)?;
            c.set_parameter(CParameter::ContentSizeFlag(false))
                .map_err(compression)?;
            c.compress(data).map_err(compression)?
        }
        Algorithm::Deflate => {
            let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(9));
            enc.write_all(data)
                .and_then(|_| enc.finish())
                .map_err(|e| Error::Compression(e.to_string()))?
        }
    };
    let to_u32 = |n: usize, what: &str| {
        u32::try_from(n).map_err(|_| format_err(format!("the {what} does not fit a DCX header")))
    };
    let mut raw = template.raw;
    raw[0x1C..0x20].copy_from_slice(&to_u32(data.len(), "uncompressed size")?.to_be_bytes());
    raw[0x20..0x24].copy_from_slice(&to_u32(body.len(), "compressed size")?.to_be_bytes());
    if template.algorithm == Algorithm::Zstd {
        raw[0x30] = ZSTD_LEVEL as u8;
    }
    let mut out = Vec::with_capacity(HEADER_LEN + body.len() + 16);
    out.extend_from_slice(&raw);
    out.extend_from_slice(&body);
    while !out.len().is_multiple_of(16) {
        out.push(0);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WRITABLE: [Algorithm; 2] = [Algorithm::Zstd, Algorithm::Deflate];

    fn sample() -> Vec<u8> {
        (0..50_000u32).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn what_is_written_reads_back_and_a_second_write_is_identical() {
        let data = sample();
        for alg in WRITABLE {
            let packed = compress(&Header::new(alg), &data).unwrap();
            let (header, back) = decompress(&packed).unwrap();
            assert_eq!(header.algorithm, alg);
            assert_eq!(header.uncompressed_size(), data.len());
            assert_eq!(back, data);
            assert_eq!(compress(&header, &back).unwrap(), packed, "byte for byte");
        }
    }

    #[test]
    fn a_kraken_header_is_kept_when_rewritten_as_zstd() {
        let kraken = Header::new(Algorithm::Kraken);
        assert!(
            compress(&kraken, b"abc").is_err(),
            "Kraken is never written"
        );
        let packed = compress(&kraken.with_algorithm(Algorithm::Zstd), b"abc").unwrap();
        let (header, back) = decompress(&packed).unwrap();
        assert_eq!(header.algorithm, Algorithm::Zstd);
        assert_eq!(back, b"abc");
        assert_eq!(&header.raw()[..0x18], &kraken.raw()[..0x18]);
    }

    /// The frame the game accepts for `regulation.bin`: a 64 KiB window, no
    /// content size, and the container padded to 16 bytes.
    #[test]
    fn a_zstd_frame_is_written_the_way_the_game_accepts() {
        let data: Vec<u8> = (0..200_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let packed = compress(&Header::new(Algorithm::Zstd), &data).unwrap();
        let csize = u32::from_be_bytes(packed[0x20..0x24].try_into().unwrap()) as usize;
        let tail = &packed[HEADER_LEN + csize..];
        assert!(packed.len().is_multiple_of(16));
        assert!(tail.len() < 16 && tail.iter().all(|&b| b == 0));
        let frame = &packed[HEADER_LEN..];
        assert_eq!(&frame[..4], &[0x28, 0xB5, 0x2F, 0xFD]);
        let descriptor = frame[4];
        assert_eq!(
            descriptor & 0x20,
            0,
            "not single-segment, so a window is declared"
        );
        assert_eq!(descriptor >> 6, 0, "no content size in the frame");
        let exponent = u32::from(frame[5] >> 3);
        let mantissa = u32::from(frame[5] & 7);
        let base = 1u64 << (10 + exponent);
        assert_eq!(
            base + (base / 8) * u64::from(mantissa),
            1 << ZSTD_WINDOW_LOG
        );
    }

    #[test]
    fn an_impossible_size_claim_is_refused_before_anything_is_allocated() {
        for alg in WRITABLE {
            let mut packed = compress(&Header::new(alg), b"small").unwrap();
            packed[0x1C..0x20].copy_from_slice(&u32::MAX.to_be_bytes());
            assert!(decompress(&packed).is_err());
            assert!(
                decompress_limited(&compress(&Header::new(alg), b"small").unwrap(), 2).is_err()
            );
        }
    }

    #[test]
    fn a_zlib_stream_longer_than_its_claim_is_refused() {
        let mut packed = compress(&Header::new(Algorithm::Deflate), &vec![0u8; 1 << 20]).unwrap();
        packed[0x1C..0x20].copy_from_slice(&1024u32.to_be_bytes());
        assert!(decompress(&packed).is_err());
    }

    #[test]
    fn an_unknown_compression_or_layout_is_named_rather_than_guessed_at() {
        let mut packed = compress(&Header::new(Algorithm::Zstd), b"abc").unwrap();
        packed[0x28..0x2C].copy_from_slice(b"EDGE");
        let err = decompress(&packed).unwrap_err().to_string();
        assert!(err.contains("EDGE"), "{err}");
        let mut packed = compress(&Header::new(Algorithm::Zstd), b"abc").unwrap();
        packed[0x14..0x18].copy_from_slice(&0x50u32.to_be_bytes());
        assert!(decompress(&packed).is_err());
        // The second set the game's deflate files store is accepted.
        let mut packed = compress(&Header::new(Algorithm::Deflate), b"abc").unwrap();
        packed[0x10..0x14].copy_from_slice(&0x24u32.to_be_bytes());
        packed[0x14..0x18].copy_from_slice(&0x2Cu32.to_be_bytes());
        assert_eq!(decompress(&packed).unwrap().1, b"abc");
    }

    #[test]
    fn every_truncation_is_an_error_and_never_a_panic() {
        for alg in WRITABLE {
            let packed = compress(&Header::new(alg), &sample()).unwrap();
            let csize = u32::from_be_bytes(packed[0x20..0x24].try_into().unwrap()) as usize;
            for len in (0..HEADER_LEN + csize).step_by(97) {
                assert!(decompress(&packed[..len]).is_err(), "{alg:?} cut at {len}");
            }
        }
    }

    #[cfg(not(feature = "kraken"))]
    #[test]
    fn without_the_feature_kraken_is_refused_by_name() {
        let mut packed = compress(&Header::new(Algorithm::Zstd), b"abc").unwrap();
        packed[0x28..0x2C].copy_from_slice(b"KRAK");
        let err = decompress(&packed).unwrap_err().to_string();
        assert!(err.contains("Kraken"), "{err}");
    }
    /// Random damage to a valid file: every outcome must be a value or an
    /// error, never a panic. A small xorshift keeps it repeatable without a
    /// dependency.
    #[test]
    fn mutated_files_never_panic() {
        let base = compress(&Header::new(Algorithm::Zstd), &sample()).unwrap();
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..5000 {
            let mut b = base.clone();
            for _ in 0..1 + next() % 8 {
                let i = (next() as usize) % b.len();
                b[i] = next() as u8;
            }
            if next() % 4 == 0 {
                b.truncate((next() as usize) % b.len());
            }
            let _ = decompress_limited(&b, 1 << 20);
        }
    }
}
