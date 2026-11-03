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

/// A number as the game's scripts write one, with no end: one byte for -64 to
/// 63, else [`INT`] and four bytes (`1` is `[0x41]`, `100000` is
/// `[0x82, 0xA0, 0x86, 0x01, 0x00]`).
pub fn int(v: i32) -> Vec<u8> {
    if (-SMALL_BIAS..i32::from(SMALL_LIMIT) - SMALL_BIAS).contains(&v) {
        vec![(v + SMALL_BIAS) as u8]
    } else {
        let mut b = vec![INT];
        b.extend_from_slice(&v.to_le_bytes());
        b
    }
}

/// A plain number as a whole expression, as a command's argument.
pub fn number(v: i32) -> Vec<u8> {
    let mut b = int(v);
    b.push(END);
    b
}

/// Calls follow the scripts' own bytes: the function's number, its arguments,
/// then 0x84 plus how many. In a condition each call and each "and"'s left side
/// is followed by 0xA6, as the game's conditions have it; in a command's
/// argument they are not (`f15(f) == 1` is `4f 82 … 85 41 95` there).
const CALL: u8 = 0x84;
const AFTER: u8 = 0xA6;
const EQ: u8 = 0x95;
const AND: u8 = 0x98;

/// Where an expression is used, which decides how its calls are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Argument,
    Condition,
}

/// `f(args…) == value`, with no end.
pub fn call_is(place: Place, function: i32, args: &[i32], value: i32) -> Vec<u8> {
    let mut b = int(function);
    for a in args {
        b.extend(int(*a));
    }
    b.push(CALL + args.len() as u8);
    if place == Place::Condition {
        b.push(AFTER);
    }
    b.extend(int(value));
    b.push(EQ);
    b
}

/// Every part joined by "and", ended. The game marks an "and"'s left side with
/// 0xA6 in arguments and conditions alike (the Twin Maiden Husks' `f15(…) == 1
/// && f15(…) == 1` is `4f 82 … 85 41 95 a6 4f 82 … 85 41 95 98`).
pub fn all(parts: &[Vec<u8>]) -> Vec<u8> {
    let mut b = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        b.extend_from_slice(p);
        if i > 0 {
            b.push(AND);
        }
        if i + 1 < parts.len() {
            b.push(AFTER);
        }
    }
    b.push(END);
    b
}

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
    fn numbers_and_tests_are_written_as_the_game_writes_them() {
        assert_eq!(int(1), vec![0x41]);
        assert_eq!(int(-1), vec![0x3F]);
        assert_eq!(int(63), vec![0x7F]);
        assert_eq!(int(64), vec![INT, 64, 0, 0, 0]);
        assert_eq!(
            int(-65),
            [vec![INT], (-65i32).to_le_bytes().to_vec()].concat()
        );
        for v in [-100, -64, 0, 63, 64, 100_000, 23_330_000] {
            assert_eq!(constant(&number(v)), Some(v), "{v}");
        }
        // Thops's menu, `f15(1039399210) == 1` as an argument.
        let mut flag = vec![0x4F, INT];
        flag.extend(1_039_399_210i32.to_le_bytes());
        flag.extend([0x85, 0x41, 0x95, END]);
        assert_eq!(
            all(&[call_is(Place::Argument, 15, &[1_039_399_210], 1)]),
            flag
        );
        // `f19() == 0` as a condition, and the Husks' and.
        assert_eq!(
            all(&[call_is(Place::Condition, 19, &[], 0)]),
            vec![0x53, 0x84, 0xA6, 0x40, 0x95, END]
        );
        let both = all(&[
            call_is(Place::Argument, 15, &[11_109_744], 1),
            call_is(Place::Argument, 15, &[1_038_519_257], 1),
        ]);
        assert_eq!(
            both,
            vec![
                0x4F, 0x82, 0x70, 0x85, 0xA9, 0x00, 0x85, 0x41, 0x95, 0xA6, 0x4F, 0x82, 0xD9, 0x8B,
                0xE6, 0x3D, 0x85, 0x41, 0x95, 0x98, END
            ]
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
