//! ESD expressions: small stack programs that compute a command's arguments
//! and a condition's test. Only what the randomizer needs is read here: an
//! argument that is a plain number.
//!
//! As the game's scripts show it (`fs-tool/tests/real_talk.rs` re-checks):
//! - every expression ends with 0xA1;
//! - 0x82 is followed by a 4-byte little-endian integer;
//! - a byte below 0x80 is a small integer, the byte minus 64;
//! - 0xB8 takes the number before it as an index into the arguments the
//!   state machine was called with (`[0x40 + k, 0xB8, 0xA1]` is argument k).

/// The end of every expression.
pub const END: u8 = 0xA1;
/// A 4-byte integer follows.
pub const INT: u8 = 0x82;
/// The argument whose index is the number before it.
pub const ARG: u8 = 0xB8;
/// Bytes below this are small integers, offset by 64.
const SMALL_LIMIT: u8 = 0x80;
const SMALL_BIAS: i32 = 64;

/// The number an expression is, when it is nothing but one number.
pub fn constant(e: &[u8]) -> Option<i32> {
    match e {
        [INT, a, b, c, d, END] => Some(i32::from_le_bytes([*a, *b, *c, *d])),
        [small, END] if *small < SMALL_LIMIT => Some(i32::from(*small) - SMALL_BIAS),
        _ => None,
    }
}

/// The index of the call argument an expression passes on, when it is
/// nothing but one.
pub fn argument(e: &[u8]) -> Option<usize> {
    match e {
        [small, ARG, END] if *small < SMALL_LIMIT => {
            usize::try_from(i32::from(*small) - SMALL_BIAS).ok()
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_are_read_and_anything_else_is_not() {
        assert_eq!(
            constant(&[0x82, 0xA0, 0x86, 0x01, 0x00, 0xA1]),
            Some(100_000)
        );
        assert_eq!(constant(&[0x41, 0xA1]), Some(1));
        assert_eq!(constant(&[0x40, 0xA1]), Some(0));
        assert_eq!(
            constant(&[0x41, 0x41, 0x8C, 0xA1]),
            None,
            "a sum is not a constant"
        );
        assert_eq!(constant(&[0x82, 0, 0, 0, 0]), None, "no end");
        assert_eq!(constant(&[]), None);
        assert_eq!(
            constant(&[0x42, 0xB8, 0xA1]),
            None,
            "an argument is not a constant"
        );
    }

    #[test]
    fn an_argument_is_read_by_its_index() {
        assert_eq!(argument(&[0x42, 0xB8, 0xA1]), Some(2));
        assert_eq!(argument(&[0x40, 0xB8, 0xA1]), Some(0));
        assert_eq!(argument(&[0x3F, 0xB8, 0xA1]), None, "no negative index");
        assert_eq!(argument(&[0x41, 0xA1]), None);
    }
}
