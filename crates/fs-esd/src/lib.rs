//! ESD: the talk scripts behind NPC dialogue, shops and gifts. Each is a set
//! of state machines: states, conditions that move between them, and commands
//! (with arguments as small expression programs) run on the way.
//!
//! Observed on Elden Ring 1.17.2's `script/talk/*.talkesdbnd.dcx`
//! (little-endian, 64-bit offsets). The file is a 0x6C-byte header, then data
//! that every offset is relative to:
//!
//! ```text
//! header  0x00 "fsSL" 1 3 3  0x10 0x54  0x14 data size  0x18 6
//!         0x1C (size, count) of: the script header (0x48, 1), state groups
//!         (0x20), states (0x48), conditions (0x38), commands (0x18) and
//!         command arguments (0x10);
//!         0x4C condition-list offset, count; 0x54 end of the expressions;
//!         0x58 the name's length; 0x5C data size, 0, data size, 0
//! data    script header (0x48): 1, a 16-byte hash, 0, state groups
//!         (offset, count), name (offset, length), -1, -1
//!         then each table in the order above, the condition list (i64
//!         offsets of conditions), the expression bytes, and the name (UTF-16,
//!         at an even offset)
//! ```
//!
//! The tables are fixed-size records whose offsets point into each other, so
//! they are kept as read; the header is rebuilt from them. `apoc-fs check`
//! holds every ESD in the game to coming back byte for byte.

pub mod calls;
pub mod commands;
pub mod expr;
pub mod handover;

use fs_bytes::{self as rd, Error, Result, format_err};

const MAGIC: &[u8; 4] = b"fsSL";
const HEADER_LEN: usize = 0x6C;
const SCRIPT_LEN: usize = 0x48;
const GROUP_LEN: usize = 0x20;
const STATE_LEN: usize = 0x48;
const CONDITION_LEN: usize = 0x38;
const COMMAND_LEN: usize = 0x18;
const ARG_LEN: usize = 0x10;
/// The longest name accepted, in UTF-16 units.
const MAX_NAME: usize = 256;

/// An offset into the data and a count (of records, or of bytes for an
/// expression). An empty list is (-1, 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub offset: i64,
    pub count: i64,
}

impl Span {
    fn read(b: &[u8], at: usize) -> Result<Self> {
        Ok(Span {
            offset: rd::i64_le(b, at, "ESD offset")?,
            count: rd::i64_le(b, at + 8, "ESD count")?,
        })
    }

    fn write(self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.offset.to_le_bytes());
        out.extend_from_slice(&self.count.to_le_bytes());
    }
}

/// A state machine: its id and its states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub id: i64,
    pub states: Span,
    /// A second copy of the states' offset, as read.
    pub states_again: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub id: i64,
    /// Into the condition list.
    pub conditions: Span,
    pub on_enter: Span,
    pub on_exit: Span,
    pub on_update: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Condition {
    /// The state it moves to, or -1.
    pub next_state: i64,
    pub on_pass: Span,
    /// Into the condition list.
    pub subconditions: Span,
    pub evaluator: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub bank: i32,
    pub id: i32,
    pub args: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Esd {
    /// Header word 0x18, 6 in every file seen.
    pub unk18: u32,
    pub hash: [u8; 16],
    pub groups: Vec<Group>,
    pub states: Vec<State>,
    pub conditions: Vec<Condition>,
    pub commands: Vec<Command>,
    /// Each argument's expression.
    pub args: Vec<Span>,
    /// Offsets of conditions, which states and conditions list from.
    pub condition_list: Vec<i64>,
    /// Every expression's bytes; spans point into the data at `expr_start`.
    pub expressions: Vec<u8>,
    /// The script's name, e.g. `t334011000`, without its terminator.
    pub name: Vec<u16>,
}

/// Whether `b` starts like an ESD.
pub fn is_esd(b: &[u8]) -> bool {
    b.starts_with(MAGIC)
}

fn expect(b: &[u8], at: usize, want: u32, what: &str) -> Result<()> {
    let got = rd::u32_le(b, at, "ESD header")?;
    if got != want {
        return Err(Error::Unsupported(format!(
            "this ESD has {got:#x} for {what}, not {want:#x}"
        )));
    }
    Ok(())
}

impl Esd {
    pub fn read(b: &[u8]) -> Result<Self> {
        if !is_esd(b) {
            return Err(format_err("not an ESD (no fsSL)"));
        }
        for (at, want, what) in [
            (0x04, 1, "word 0x04"),
            (0x08, 3, "word 0x08"),
            (0x0C, 3, "word 0x0C"),
            (0x10, 0x54, "the header size"),
            (0x1C, SCRIPT_LEN as u32, "the script header's size"),
            (0x20, 1, "the script header count"),
            (0x24, GROUP_LEN as u32, "the state group size"),
            (0x2C, STATE_LEN as u32, "the state size"),
            (0x34, CONDITION_LEN as u32, "the condition size"),
            (0x3C, COMMAND_LEN as u32, "the command size"),
            (0x44, ARG_LEN as u32, "the argument size"),
            (0x60, 0, "word 0x60"),
            (0x68, 0, "word 0x68"),
        ] {
            expect(b, at, want, what)?;
        }
        let word = |at| rd::u32_le(b, at, "ESD header").map(|v| v as usize);
        let size = word(0x14)?;
        let d = rd::bytes(b, HEADER_LEN, size, "ESD data")?;
        if b.len() != HEADER_LEN + size {
            return Err(format_err(format!(
                "the ESD is {} bytes, not the {} its header gives",
                b.len(),
                HEADER_LEN + size
            )));
        }
        for at in [0x5C, 0x64] {
            if word(at)? != size {
                return Err(format_err(format!(
                    "ESD header word {at:#x} is not the data size"
                )));
            }
        }
        let count = |at| word(at);
        let (n_groups, n_states, n_conds, n_cmds, n_args) = (
            count(0x28)?,
            count(0x30)?,
            count(0x38)?,
            count(0x40)?,
            count(0x48)?,
        );
        let (list_at, n_list, expr_end, name_len) =
            (word(0x4C)?, count(0x50)?, word(0x54)?, word(0x58)?);

        // The script header.
        if rd::u32_le(d, 0, "ESD script header")? != 1
            || rd::u32_le(d, 0x14, "ESD script header")? != 0
        {
            return Err(Error::Unsupported(
                "this ESD's script header is unfamiliar".into(),
            ));
        }
        let hash: [u8; 16] = rd::bytes(d, 4, 16, "ESD hash")?.try_into().unwrap();
        let group_span = Span::read(d, 0x18)?;
        let name_span = Span::read(d, 0x28)?;
        if rd::i64_le(d, 0x38, "ESD script header")? != -1
            || rd::i64_le(d, 0x40, "ESD script header")? != -1
        {
            return Err(Error::Unsupported(
                "this ESD's script header is unfamiliar".into(),
            ));
        }

        // The tables, one after another.
        let at = std::cell::Cell::new(SCRIPT_LEN);
        let table = |n: usize, len: usize, what: &'static str| -> Result<Vec<&[u8]>> {
            let total = n.checked_mul(len).ok_or(Error::Truncated(what))?;
            let all = rd::bytes(d, at.get(), total, what)?;
            at.set(at.get() + total);
            Ok(all.chunks(len).collect())
        };
        let groups = table(n_groups, GROUP_LEN, "ESD state groups")?
            .into_iter()
            .map(|r| {
                Ok(Group {
                    id: rd::i64_le(r, 0, "ESD state group")?,
                    states: Span::read(r, 8)?,
                    states_again: rd::i64_le(r, 0x18, "ESD state group")?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let states = table(n_states, STATE_LEN, "ESD states")?
            .into_iter()
            .map(|r| {
                Ok(State {
                    id: rd::i64_le(r, 0, "ESD state")?,
                    conditions: Span::read(r, 0x08)?,
                    on_enter: Span::read(r, 0x18)?,
                    on_exit: Span::read(r, 0x28)?,
                    on_update: Span::read(r, 0x38)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let conditions = table(n_conds, CONDITION_LEN, "ESD conditions")?
            .into_iter()
            .map(|r| {
                Ok(Condition {
                    next_state: rd::i64_le(r, 0, "ESD condition")?,
                    on_pass: Span::read(r, 0x08)?,
                    subconditions: Span::read(r, 0x18)?,
                    evaluator: Span::read(r, 0x28)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let commands = table(n_cmds, COMMAND_LEN, "ESD commands")?
            .into_iter()
            .map(|r| {
                Ok(Command {
                    bank: rd::i32_le(r, 0, "ESD command")?,
                    id: rd::i32_le(r, 4, "ESD command")?,
                    args: Span::read(r, 8)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let args = table(n_args, ARG_LEN, "ESD arguments")?
            .into_iter()
            .map(|r| Span::read(r, 0))
            .collect::<Result<Vec<_>>>()?;
        if list_at != at.get() {
            return Err(format_err(format!(
                "the ESD's condition list is at {list_at:#x}, not right after the tables ({:#x})",
                at.get()
            )));
        }
        let condition_list = table(n_list, 8, "ESD condition list")?
            .into_iter()
            .map(|r| rd::i64_le(r, 0, "ESD condition list"))
            .collect::<Result<Vec<_>>>()?;
        let expressions = rd::bytes(
            d,
            at.get(),
            expr_end
                .checked_sub(at.get())
                .ok_or(Error::Truncated("ESD expressions"))?,
            "ESD expressions",
        )?
        .to_vec();

        // The name: right after the expressions, at an even offset.
        let name_at = expr_end + expr_end % 2;
        let name = rd::utf16z(d, name_at, MAX_NAME, "ESD name")?;
        let esd = Esd {
            unk18: word(0x18)? as u32,
            hash,
            groups,
            states,
            conditions,
            commands,
            args,
            condition_list,
            expressions,
            name,
        };
        // Everything derived must be what the file holds.
        if name_len != esd.name.len() + 1
            || name_span
                != (Span {
                    offset: name_at as i64,
                    count: name_len as i64,
                })
            || group_span != esd.group_span()
            || size != esd.data_len()
        {
            return Err(format_err(
                "the ESD's name, state groups or size do not match their header",
            ));
        }
        Ok(esd)
    }

    fn tables_end(&self) -> usize {
        SCRIPT_LEN
            + self.groups.len() * GROUP_LEN
            + self.states.len() * STATE_LEN
            + self.conditions.len() * CONDITION_LEN
            + self.commands.len() * COMMAND_LEN
            + self.args.len() * ARG_LEN
    }

    /// Where the expressions start in the data; spans into them are offsets
    /// from the data's start.
    pub fn expr_start(&self) -> usize {
        self.tables_end() + self.condition_list.len() * 8
    }

    fn expr_end(&self) -> usize {
        self.expr_start() + self.expressions.len()
    }

    fn name_at(&self) -> usize {
        self.expr_end() + self.expr_end() % 2
    }

    fn data_len(&self) -> usize {
        self.name_at() + (self.name.len() + 1) * 2
    }

    fn group_span(&self) -> Span {
        match self.groups.len() {
            0 => Span {
                offset: -1,
                count: 0,
            },
            n => Span {
                offset: SCRIPT_LEN as i64,
                count: n as i64,
            },
        }
    }

    /// The bytes of an expression, by its span.
    pub fn expression(&self, span: Span) -> Option<&[u8]> {
        let start = usize::try_from(span.offset)
            .ok()?
            .checked_sub(self.expr_start())?;
        let len = usize::try_from(span.count).ok()?;
        self.expressions.get(start..start.checked_add(len)?)
    }

    fn states_at(&self) -> usize {
        SCRIPT_LEN + self.groups.len() * GROUP_LEN
    }

    fn conditions_at(&self) -> usize {
        self.states_at() + self.states.len() * STATE_LEN
    }

    fn commands_at(&self) -> usize {
        self.conditions_at() + self.conditions.len() * CONDITION_LEN
    }

    fn args_at(&self) -> usize {
        self.commands_at() + self.commands.len() * COMMAND_LEN
    }

    /// The records of one table that a span names, or none if it points
    /// anywhere else.
    fn records<T>(items: &[T], at: usize, len: usize, span: Span) -> &[T] {
        let Some(first) = usize::try_from(span.offset)
            .ok()
            .and_then(|o| o.checked_sub(at))
            .filter(|o| o % len == 0)
            .map(|o| o / len)
        else {
            return &[];
        };
        let n = usize::try_from(span.count).unwrap_or(0);
        items
            .get(first..)
            .map_or(&[], |rest| &rest[..n.min(rest.len())])
    }

    /// The states of a state machine.
    pub fn states_of(&self, g: &Group) -> &[State] {
        Self::records(&self.states, self.states_at(), STATE_LEN, g.states)
    }

    /// The state a condition moves to, by its offset, with its index.
    pub fn state_at(&self, offset: i64) -> Option<(usize, &State)> {
        let i = usize::try_from(offset)
            .ok()?
            .checked_sub(self.states_at())?;
        if i % STATE_LEN != 0 {
            return None;
        }
        self.states.get(i / STATE_LEN).map(|s| (i / STATE_LEN, s))
    }

    /// The commands a span names.
    pub fn commands_in(&self, span: Span) -> &[Command] {
        Self::records(&self.commands, self.commands_at(), COMMAND_LEN, span)
    }

    /// The conditions a span into the condition list names.
    pub fn conditions_in(&self, span: Span) -> Vec<&Condition> {
        Self::records(&self.condition_list, self.tables_end(), 8, span)
            .iter()
            .filter_map(|&o| {
                let i = usize::try_from(o).ok()?.checked_sub(self.conditions_at())?;
                (i % CONDITION_LEN == 0)
                    .then(|| self.conditions.get(i / CONDITION_LEN))
                    .flatten()
            })
            .collect()
    }

    /// A command's arguments, each as its expression's bytes.
    pub fn command_args(&self, c: &Command) -> Vec<Option<&[u8]>> {
        Self::records(&self.args, self.args_at(), ARG_LEN, c.args)
            .iter()
            .map(|&s| self.expression(s))
            .collect()
    }

    pub fn name(&self) -> String {
        String::from_utf16_lossy(&self.name)
    }

    pub fn write(&self) -> Result<Vec<u8>> {
        let size = self.data_len();
        let u32_of = |n: usize, what: &str| {
            u32::try_from(n).map_err(|_| format_err(format!("the ESD's {what} is too large")))
        };
        let mut out = Vec::with_capacity(HEADER_LEN + size);
        out.extend_from_slice(MAGIC);
        for (v, what) in [
            (1, ""),
            (3, ""),
            (3, ""),
            (0x54, ""),
            (size, "size"),
            (self.unk18 as usize, ""),
            (SCRIPT_LEN, ""),
            (1, ""),
            (GROUP_LEN, ""),
            (self.groups.len(), "state groups"),
            (STATE_LEN, ""),
            (self.states.len(), "states"),
            (CONDITION_LEN, ""),
            (self.conditions.len(), "conditions"),
            (COMMAND_LEN, ""),
            (self.commands.len(), "commands"),
            (ARG_LEN, ""),
            (self.args.len(), "arguments"),
            (self.tables_end(), "tables"),
            (self.condition_list.len(), "condition list"),
            (self.expr_end(), "expressions"),
            (self.name.len() + 1, "name"),
            (size, "size"),
            (0, ""),
            (size, "size"),
            (0, ""),
        ] {
            out.extend_from_slice(&u32_of(v, what)?.to_le_bytes());
        }
        debug_assert_eq!(out.len(), HEADER_LEN);

        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&self.hash);
        out.extend_from_slice(&0u32.to_le_bytes());
        self.group_span().write(&mut out);
        Span {
            offset: self.name_at() as i64,
            count: self.name.len() as i64 + 1,
        }
        .write(&mut out);
        out.extend_from_slice(&(-1i64).to_le_bytes());
        out.extend_from_slice(&(-1i64).to_le_bytes());
        for g in &self.groups {
            out.extend_from_slice(&g.id.to_le_bytes());
            g.states.write(&mut out);
            out.extend_from_slice(&g.states_again.to_le_bytes());
        }
        for s in &self.states {
            out.extend_from_slice(&s.id.to_le_bytes());
            for span in [s.conditions, s.on_enter, s.on_exit, s.on_update] {
                span.write(&mut out);
            }
        }
        for c in &self.conditions {
            out.extend_from_slice(&c.next_state.to_le_bytes());
            for span in [c.on_pass, c.subconditions, c.evaluator] {
                span.write(&mut out);
            }
        }
        for c in &self.commands {
            out.extend_from_slice(&c.bank.to_le_bytes());
            out.extend_from_slice(&c.id.to_le_bytes());
            c.args.write(&mut out);
        }
        for a in &self.args {
            a.write(&mut out);
        }
        for o in &self.condition_list {
            out.extend_from_slice(&o.to_le_bytes());
        }
        out.extend_from_slice(&self.expressions);
        if self.expr_end() % 2 == 1 {
            out.push(0);
        }
        for u in &self.name {
            out.extend_from_slice(&u.to_le_bytes());
        }
        out.extend_from_slice(&[0, 0]);
        debug_assert_eq!(out.len(), HEADER_LEN + size);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A script with one state that runs one command with one argument, the
    /// constant 100000, and an odd-length expression area so the name needs
    /// a pad byte.
    fn sample() -> Esd {
        let mut esd = Esd {
            unk18: 6,
            hash: [7; 16],
            groups: vec![],
            states: vec![],
            conditions: vec![],
            commands: vec![],
            args: vec![],
            condition_list: vec![],
            expressions: vec![0x82, 0xA0, 0x86, 0x01, 0x00, 0xA1, 0x41],
            name: "t100001000".encode_utf16().collect(),
        };
        let groups_at = SCRIPT_LEN as i64;
        let states_at = groups_at + GROUP_LEN as i64;
        let commands_at = states_at + STATE_LEN as i64;
        let args_at = commands_at + COMMAND_LEN as i64;
        let expr_at = args_at + ARG_LEN as i64;
        let none = Span {
            offset: -1,
            count: 0,
        };
        esd.groups.push(Group {
            id: 0x7FFF_FFFF,
            states: Span {
                offset: states_at,
                count: 1,
            },
            states_again: states_at,
        });
        esd.states.push(State {
            id: 0,
            conditions: none,
            on_enter: Span {
                offset: commands_at,
                count: 1,
            },
            on_exit: none,
            on_update: none,
        });
        esd.commands.push(Command {
            bank: 1,
            id: 104,
            args: Span {
                offset: args_at,
                count: 1,
            },
        });
        esd.args.push(Span {
            offset: expr_at,
            count: 6,
        });
        esd
    }

    #[test]
    fn a_script_reads_back_what_was_written_and_a_second_write_is_identical() {
        let bytes = sample().write().unwrap();
        let back = Esd::read(&bytes).unwrap();
        assert_eq!(back, sample());
        assert_eq!(back.write().unwrap(), bytes);
        assert_eq!(back.name(), "t100001000");
        assert_eq!(bytes.len() % 2, 0, "the name sits on an even offset");
    }

    #[test]
    fn a_commands_arguments_are_its_expressions() {
        let esd = sample();
        let args = esd.command_args(&esd.commands[0]);
        assert_eq!(args, vec![Some(&[0x82, 0xA0, 0x86, 0x01, 0x00, 0xA1][..])]);
        assert_eq!(expr::constant(args[0].unwrap()), Some(100_000));
    }

    #[test]
    fn an_empty_script_round_trips() {
        let mut esd = sample();
        esd.groups.clear();
        esd.states.clear();
        esd.commands.clear();
        esd.args.clear();
        esd.expressions.clear();
        let bytes = esd.write().unwrap();
        assert_eq!(Esd::read(&bytes).unwrap(), esd);
    }

    #[test]
    fn every_truncation_is_an_error() {
        let bytes = sample().write().unwrap();
        for n in 0..bytes.len() {
            assert!(Esd::read(&bytes[..n]).is_err(), "cut at {n}");
        }
    }

    #[test]
    fn mutated_files_never_panic() {
        let base = sample().write().unwrap();
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..20000 {
            let mut b = base.clone();
            for _ in 0..1 + next() % 8 {
                let i = (next() as usize) % b.len();
                b[i] = next() as u8;
            }
            if next() % 4 == 0 {
                b.truncate((next() as usize) % b.len());
            }
            if let Ok(esd) = Esd::read(&b) {
                for c in &esd.commands {
                    for a in esd.command_args(c).into_iter().flatten() {
                        let _ = expr::constant(a);
                    }
                }
            }
        }
    }
}
