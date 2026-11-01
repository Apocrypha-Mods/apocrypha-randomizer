//! What a condition asks for, as far as the randomizer needs it: the event
//! flags that must be on or off and the items that must be held, in the
//! parts of a condition's expression that are joined by "and".
//!
//! As the game's scripts show it (`fs-tool/tests/real_talk.rs` re-checks), an
//! expression is a stack program (see [`crate::expr`]) with:
//! - 0x84 to 0x8A: call the function whose number is pushed first, with 0 to
//!   6 arguments pushed after it;
//! - 0x8C, 0x8E, 0x8F: add, subtract, multiply; 0x91 to 0x96: <=, >=, <, >,
//!   ==, !=; 0x98, 0x99: and, or;
//! - 0xA6 to 0xAD: store the value on top in register 0 to 7, leaving it;
//!   0xAF to 0xB6: push register 0 to 7; 0xB7: marks the end of an "and"'s
//!   left side, leaving the stack alone;
//! - 0xB9, 0xBA: push a value of the machine's own (`[0xB9, 0xBA, 0x96]`,
//!   the commonest condition, compares the two);
//! - 0x80: a 4-byte float follows; 0x81: an 8-byte double.
//!
//! Two functions matter here. Function 15 takes one argument, an event flag,
//! in 16,544 calls ([`FLAG`]). Function 47 takes five: an item type (as in
//! ShopLineupParam), an item id, a comparison, a count and a fifth word
//! ([`HOLDS`]); `(3, id, 4, 1, 0)` is "holds at least one", `(3, id, 0, 0, 0)`
//! "holds none".

use crate::expr::{ARG, END, INT};

/// Function 15: whether an event flag is on.
pub const FLAG: i32 = 15;
/// Function 47: how many of an item the player holds, compared with a count.
pub const HOLDS: i32 = 47;
/// Function 47's comparison meaning "at least".
const AT_LEAST: i32 = 4;
/// Function 47's comparison meaning "exactly".
const EXACTLY: i32 = 0;

const CALL_FIRST: u8 = 0x84;
const CALL_LAST: u8 = 0x8A;
const STORE_FIRST: u8 = 0xA6;
const STORE_LAST: u8 = 0xAD;
const LOAD_FIRST: u8 = 0xAF;
const LOAD_LAST: u8 = 0xB6;
const AND_LEFT: u8 = 0xB7;
const FLOAT: u8 = 0x80;
const DOUBLE: u8 = 0x81;
const MACHINE_FIRST: u8 = 0xB9;
const MACHINE_LAST: u8 = 0xBA;
const SMALL_LIMIT: u8 = 0x80;
const SMALL_BIAS: i32 = 64;

/// A condition's expression as a tree.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    Int(i32),
    Call(i32, Vec<Node>),
    Op(u8, Box<Node>, Box<Node>),
    /// Anything not followed: a register, an argument, a float.
    Other,
}

/// One thing a condition asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Atom {
    /// An event flag on (true) or off (false).
    Flag(u32, bool),
    /// At least one of an item held: its type (3 for goods) and id.
    Holds(i32, i32),
    /// None of an item held.
    HoldsNone(i32, i32),
}

fn parse(e: &[u8]) -> Option<Node> {
    let mut stack: Vec<Node> = Vec::new();
    let mut i = 0;
    while i < e.len() {
        let b = e[i];
        i += 1;
        match b {
            END => break,
            INT => {
                let v = i32::from_le_bytes(e.get(i..i + 4)?.try_into().ok()?);
                stack.push(Node::Int(v));
                i += 4;
            }
            FLOAT => {
                i += 4;
                stack.push(Node::Other);
            }
            DOUBLE => {
                i += 8;
                stack.push(Node::Other);
            }
            MACHINE_FIRST..=MACHINE_LAST => stack.push(Node::Other),
            _ if b < SMALL_LIMIT => stack.push(Node::Int(i32::from(b) - SMALL_BIAS)),
            CALL_FIRST..=CALL_LAST => {
                let n = usize::from(b - CALL_FIRST);
                let args = stack.split_off(stack.len().checked_sub(n)?);
                match stack.pop()? {
                    Node::Int(f) => stack.push(Node::Call(f, args)),
                    _ => return None,
                }
            }
            0x8C | 0x8E | 0x8F | 0x91..=0x96 | 0x98 | 0x99 => {
                let r = stack.pop()?;
                let l = stack.pop()?;
                stack.push(Node::Op(b, Box::new(l), Box::new(r)));
            }
            STORE_FIRST..=STORE_LAST | AND_LEFT => {}
            LOAD_FIRST..=LOAD_LAST => stack.push(Node::Other),
            ARG => {
                stack.pop()?;
                stack.push(Node::Other);
            }
            _ => return None,
        }
    }
    match &stack[..] {
        [_] => stack.pop(),
        _ => None,
    }
}

const EQ: u8 = 0x95;
const NE: u8 = 0x96;
const AND: u8 = 0x98;

/// Whether an expression reads as one value: every opcode known and the
/// stack left with one thing on it.
pub fn understood(e: &[u8]) -> bool {
    parse(e).is_some()
}

/// What an expression asks for in every case where it is true: the atoms
/// of the parts joined by "and". Anything else, an "or" among them, adds
/// nothing.
pub fn requires(e: &[u8]) -> Vec<Atom> {
    let mut out = Vec::new();
    if let Some(n) = parse(e) {
        collect(&n, true, &mut out);
    }
    out
}

/// What is sure when an expression is false: of a plain test, its opposite. A
/// failed "and" says nothing of which part failed, so it adds nothing.
pub fn refuses(e: &[u8]) -> Vec<Atom> {
    let mut out = Vec::new();
    if let Some(n) = parse(e) {
        collect(&n, false, &mut out);
    }
    out
}

/// The atoms `n` asks for when it is `truth`.
fn collect(n: &Node, truth: bool, out: &mut Vec<Atom>) {
    match n {
        Node::Op(AND, l, r) if truth => {
            collect(l, true, out);
            collect(r, true, out);
        }
        Node::Op(op @ (EQ | NE), l, r) => {
            let (call, v) = match (&**l, &**r) {
                (c, Node::Int(v)) | (Node::Int(v), c) => (c, *v),
                _ => return,
            };
            // `x == 0` and `x != 1` are "x is false", for truth values.
            let is = (*op == EQ) == truth;
            if matches!(call, Node::Call(..) | Node::Op(..)) && (v == 0 || v == 1) {
                collect(call, is == (v == 1), out);
            }
        }
        Node::Call(FLAG, args) => {
            if let [Node::Int(f)] = &args[..]
                && let Ok(f) = u32::try_from(*f)
            {
                out.push(Atom::Flag(f, truth));
            }
        }
        Node::Call(HOLDS, args) => {
            if let [Node::Int(t), Node::Int(id), Node::Int(cmp), Node::Int(n), _] = &args[..] {
                match (*cmp, *n, truth) {
                    (AT_LEAST, 1.., true) => out.push(Atom::Holds(*t, *id)),
                    (EXACTLY, 0, true) => out.push(Atom::HoldsNone(*t, *id)),
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(v: i32) -> Vec<u8> {
        let mut b = vec![INT];
        b.extend_from_slice(&v.to_le_bytes());
        b
    }

    #[test]
    fn a_failed_test_says_its_opposite_and_a_failed_and_says_nothing() {
        let mut one = vec![0x4F];
        one.extend(int(77));
        one.extend([0x85, 0x41, EQ]);
        let mut e = one.clone();
        e.push(END);
        assert_eq!(refuses(&e), vec![Atom::Flag(77, false)]);
        let mut both = one.clone();
        both.extend(one);
        both.extend([AND, END]);
        assert_eq!(refuses(&both), vec![]);
    }

    #[test]
    fn a_flag_test_asks_for_the_flag() {
        // fn15(F) == 1
        let mut e = vec![0x4F];
        e.extend(int(1_044_369_244));
        e.extend([0x85, 0xA6, 0x41, EQ, END]);
        assert_eq!(requires(&e), vec![Atom::Flag(1_044_369_244, true)]);
        // fn15(F) == 0
        let n = e.len();
        e[n - 3] = 0x40;
        assert_eq!(requires(&e), vec![Atom::Flag(1_044_369_244, false)]);
    }

    #[test]
    fn and_joins_and_or_does_not() {
        // fn15(7) == 1 && fn47(3, 8850, 4, 1, 0) == 1
        let mut e = vec![0x4F, 0x47, 0x85, 0xA6, 0x41, EQ, AND_LEFT, 0x40 + 47, 0x43];
        e.extend(int(8850));
        e.extend([0x44, 0x41, 0x40, 0x89, 0xA6, 0x41, EQ, AND, END]);
        assert_eq!(
            requires(&e),
            vec![Atom::Flag(7, true), Atom::Holds(3, 8850)]
        );
        let n = e.len();
        e[n - 2] = 0x99;
        assert_eq!(requires(&e), vec![]);
    }

    #[test]
    fn not_and_is_left_alone() {
        // (fn15(7) == 1 && fn15(8) == 1) == 0: nothing is sure.
        let e = [
            0x4F, 0x47, 0x85, 0x41, EQ, 0x4F, 0x48, 0x85, 0x41, EQ, AND, 0x40, EQ, END,
        ];
        assert_eq!(requires(&e), vec![]);
    }

    #[test]
    fn mutated_expressions_never_panic() {
        // fn15(7) == 1 && fn47(3, 8850, 4, 1, 0) == 1, then mutated.
        let mut base = vec![0x4F, 0x47, 0x85, 0xA6, 0x41, EQ, AND_LEFT, 0x6F, 0x43];
        base.extend(int(8850));
        base.extend([0x44, 0x41, 0x40, 0x89, 0xA6, 0x41, EQ, AND, END]);
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..20000 {
            let mut e = base.clone();
            for _ in 0..1 + next() % 4 {
                let i = (next() as usize) % e.len();
                e[i] = next() as u8;
            }
            if next() % 4 == 0 {
                e.truncate((next() as usize) % e.len());
            }
            let _ = requires(&e);
        }
    }

    #[test]
    fn what_is_not_understood_asks_for_nothing() {
        assert_eq!(requires(&[0xFC, END]), vec![]);
        assert_eq!(requires(&[0x41, 0x41, END]), vec![]);
        assert_eq!(requires(&[]), vec![]);
    }
}
