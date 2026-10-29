//! Bounds-checked little- and big-endian reads, and the error every format
//! crate reports.
//!
//! Lifted from `apoc-fsparams` (`rd.rs`, `Error`), Nova's own code. Every
//! offset read here comes out of a game file or somebody's mod, so none of them
//! is trusted: an index past the end is an error naming what was being read,
//! never a panic.

/// Why a file could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the {0} is cut short")]
    Truncated(&'static str),
    #[error("{0}")]
    Format(String),
    #[error("could not decrypt: {0}")]
    Crypto(String),
    #[error("could not decompress: {0}")]
    Compression(String),
    #[error("{0}")]
    Unsupported(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// A format error, from anything that displays.
pub fn format_err(msg: impl Into<String>) -> Error {
    Error::Format(msg.into())
}

pub fn bytes<'a>(b: &'a [u8], at: usize, len: usize, what: &'static str) -> Result<&'a [u8]> {
    at.checked_add(len)
        .and_then(|end| b.get(at..end))
        .ok_or(Error::Truncated(what))
}

fn array<const N: usize>(b: &[u8], at: usize, what: &'static str) -> Result<[u8; N]> {
    Ok(bytes(b, at, N, what)?.try_into().unwrap())
}

pub fn u8_at(b: &[u8], at: usize, what: &'static str) -> Result<u8> {
    Ok(bytes(b, at, 1, what)?[0])
}

pub fn u16_le(b: &[u8], at: usize, what: &'static str) -> Result<u16> {
    array(b, at, what).map(u16::from_le_bytes)
}

pub fn u32_le(b: &[u8], at: usize, what: &'static str) -> Result<u32> {
    array(b, at, what).map(u32::from_le_bytes)
}

pub fn i32_le(b: &[u8], at: usize, what: &'static str) -> Result<i32> {
    array(b, at, what).map(i32::from_le_bytes)
}

pub fn u64_le(b: &[u8], at: usize, what: &'static str) -> Result<u64> {
    array(b, at, what).map(u64::from_le_bytes)
}

pub fn i64_le(b: &[u8], at: usize, what: &'static str) -> Result<i64> {
    array(b, at, what).map(i64::from_le_bytes)
}

pub fn u32_be(b: &[u8], at: usize, what: &'static str) -> Result<u32> {
    array(b, at, what).map(u32::from_be_bytes)
}

/// A count or offset stored as a signed value, as a usable index.
pub fn index(value: i64, what: &'static str) -> Result<usize> {
    usize::try_from(value).map_err(|_| Error::Format(format!("the {what} is negative")))
}

/// A NUL-terminated UTF-16LE string's code units, without the terminator.
///
/// Longer than `max_units` is an error. The caller says what length is
/// plausible, because a name read once per entry can be shared by thousands of
/// entries.
pub fn utf16z(b: &[u8], at: usize, max_units: usize, what: &'static str) -> Result<Vec<u16>> {
    let mut out = Vec::new();
    let mut i = at;
    loop {
        let unit = u16_le(b, i, what)?;
        if unit == 0 {
            return Ok(out);
        }
        if out.len() >= max_units {
            return Err(Error::Format(format!("the {what} is implausibly long")));
        }
        out.push(unit);
        i += 2;
    }
}

/// A NUL-terminated single-byte string, without the terminator.
pub fn asciiz(b: &[u8], at: usize, max_len: usize, what: &'static str) -> Result<Vec<u8>> {
    let rest = b.get(at..).ok_or(Error::Truncated(what))?;
    let end = rest
        .iter()
        .take(max_len + 1)
        .position(|&c| c == 0)
        .ok_or_else(|| {
            if rest.len() > max_len {
                Error::Format(format!("the {what} is implausibly long"))
            } else {
                Error::Truncated(what)
            }
        })?;
    Ok(rest[..end].to_vec())
}

/// `n` rounded up to a multiple of `align`, which must be a power of two.
pub fn align_up(n: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    (n + align - 1) & !(align - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_past_the_end_are_errors_not_panics() {
        let b = [1u8, 2, 3];
        assert!(u32_le(&b, 0, "t").is_err());
        assert!(u16_le(&b, usize::MAX, "t").is_err());
        assert_eq!(u16_le(&b, 1, "t").unwrap(), 0x0302);
    }

    #[test]
    fn strings_stop_at_their_terminator_and_refuse_to_run_on() {
        let s = [b'a', 0, b'b', 0, 0, 0];
        assert_eq!(
            utf16z(&s, 0, 8, "t").unwrap(),
            vec![u16::from(b'a'), u16::from(b'b')]
        );
        assert!(utf16z(&s, 0, 1, "t").is_err());
        assert_eq!(asciiz(b"ab\0c", 0, 8, "t").unwrap(), b"ab");
        assert!(asciiz(b"abcdef", 0, 3, "t").is_err());
        assert!(asciiz(b"ab", 0, 8, "t").is_err());
    }

    #[test]
    fn alignment_rounds_up_only_when_needed() {
        assert_eq!(align_up(0, 16), 0);
        assert_eq!(align_up(1, 16), 16);
        assert_eq!(align_up(32, 16), 32);
    }
}
