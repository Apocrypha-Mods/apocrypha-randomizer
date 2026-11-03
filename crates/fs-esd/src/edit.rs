//! Changing a talk script: records put into its tables, with every offset that
//! points at or past them moved along.
//!
//! Every offset in a script is from the data's start, and the tables follow one
//! another (states, conditions, commands, arguments, the condition list, the
//! expressions), so a record put into one table moves everything after it. An
//! offset is moved by which table it points into: past the table the records
//! went into, by their size; into it, by their size when it points at the
//! place they went or after. Nothing else changes, so a script that is changed
//! by nothing writes back byte for byte.
//!
//! A record put inside a span another span also covers would join that span
//! too, so that is refused: the game's scripts share spans (two states running
//! the same commands), and a change to one must not reach the other.

use fs_bytes::{Result, format_err};

use crate::{
    ARG_LEN, COMMAND_LEN, CONDITION_LEN, Command, Condition, Esd, NO_SPAN, STATE_LEN, Span, State,
};

/// The tables records can be put into, in the order they are stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Table {
    States,
    Conditions,
    Commands,
    Args,
    ConditionList,
}

impl Table {
    const ALL: [Table; 5] = [
        Table::States,
        Table::Conditions,
        Table::Commands,
        Table::Args,
        Table::ConditionList,
    ];

    fn record_len(self) -> usize {
        match self {
            Table::States => STATE_LEN,
            Table::Conditions => CONDITION_LEN,
            Table::Commands => COMMAND_LEN,
            Table::Args => ARG_LEN,
            Table::ConditionList => 8,
        }
    }

    fn index(self) -> usize {
        Table::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }
}

impl Esd {
    /// Where each table starts in the data, then where the expressions start.
    fn starts(&self) -> [usize; 6] {
        [
            self.states_at(),
            self.conditions_at(),
            self.commands_at(),
            self.args_at(),
            self.tables_end(),
            self.expr_start(),
        ]
    }

    /// Where a record of a table is, by its index.
    pub fn offset_of(&self, table: Table, index: usize) -> i64 {
        (self.starts()[table.index()] + index * table.record_len()) as i64
    }

    /// The index in its table of the record an offset points at.
    fn index_at(&self, table: Table, offset: i64) -> Option<usize> {
        let rel = usize::try_from(offset)
            .ok()?
            .checked_sub(self.starts()[table.index()])?;
        (rel % table.record_len() == 0).then_some(rel / table.record_len())
    }

    /// Every offset the script stores, each given to `f` to move.
    fn each_offset(&mut self, mut f: impl FnMut(&mut i64)) {
        for g in &mut self.groups {
            f(&mut g.states.offset);
            f(&mut g.states_again);
        }
        for s in &mut self.states {
            for span in [
                &mut s.conditions,
                &mut s.on_enter,
                &mut s.on_exit,
                &mut s.on_update,
            ] {
                f(&mut span.offset);
            }
        }
        for c in &mut self.conditions {
            f(&mut c.next_state);
            for span in [&mut c.on_pass, &mut c.subconditions, &mut c.evaluator] {
                f(&mut span.offset);
            }
        }
        for c in &mut self.commands {
            f(&mut c.args.offset);
        }
        for a in &mut self.args {
            f(&mut a.offset);
        }
        for o in &mut self.condition_list {
            f(o);
        }
    }

    /// Put `n` empty records into `table` at index `at`, moving every offset at
    /// or past them. The records are for the caller to fill.
    pub fn insert(&mut self, table: Table, at: usize, n: usize) -> Result<()> {
        let len = match table {
            Table::States => self.states.len(),
            Table::Conditions => self.conditions.len(),
            Table::Commands => self.commands.len(),
            Table::Args => self.args.len(),
            Table::ConditionList => self.condition_list.len(),
        };
        if at > len {
            return Err(format_err(format!(
                "no place {at} in a table of {len} {table:?}"
            )));
        }
        if n == 0 {
            return Ok(());
        }
        let starts = self.starts();
        let t = table.index();
        let grow = (n * table.record_len()) as i64;
        let at_offset = (starts[t] + at * table.record_len()) as i64;
        self.each_offset(|o| {
            if *o < starts[0] as i64 {
                return;
            }
            // Past the table, or into it at or after the records.
            let k = starts.iter().rposition(|&s| *o >= s as i64).unwrap_or(0);
            if k > t || (k == t && *o >= at_offset) {
                *o += grow;
            }
        });
        let at_range = at..at;
        match table {
            Table::States => {
                self.states.splice(
                    at_range,
                    (0..n).map(|_| State {
                        id: 0,
                        conditions: NO_SPAN,
                        on_enter: NO_SPAN,
                        on_exit: NO_SPAN,
                        on_update: NO_SPAN,
                    }),
                );
            }
            Table::Conditions => {
                self.conditions.splice(
                    at_range,
                    (0..n).map(|_| Condition {
                        next_state: -1,
                        on_pass: NO_SPAN,
                        subconditions: NO_SPAN,
                        evaluator: NO_SPAN,
                    }),
                );
            }
            Table::Commands => {
                self.commands.splice(
                    at_range,
                    (0..n).map(|_| Command {
                        bank: 0,
                        id: 0,
                        args: NO_SPAN,
                    }),
                );
            }
            Table::Args => {
                self.args.splice(at_range, (0..n).map(|_| NO_SPAN));
            }
            Table::ConditionList => {
                self.condition_list.splice(at_range, (0..n).map(|_| -1));
            }
        }
        Ok(())
    }

    /// An expression added after the others; its span.
    pub fn push_expression(&mut self, bytes: &[u8]) -> Span {
        let offset = (self.expr_start() + self.expressions.len()) as i64;
        self.expressions.extend_from_slice(bytes);
        Span {
            offset,
            count: bytes.len() as i64,
        }
    }

    /// The spans into one table, as (first index, count), each with whether it
    /// is the span `mine` picks out.
    fn spans_into(&self, table: Table, mine: Option<(usize, Slot)>) -> Vec<(usize, usize, bool)> {
        let mut out = Vec::new();
        let mut add = |s: Span, is_mine: bool| {
            if s.count > 0
                && let Some(i) = self.index_at(table, s.offset)
            {
                out.push((i, s.count as usize, is_mine));
            }
        };
        for (si, s) in self.states.iter().enumerate() {
            let me = |slot| mine == Some((si, slot));
            match table {
                Table::Commands => {
                    add(s.on_enter, me(Slot::Enter));
                    add(s.on_exit, me(Slot::Exit));
                    add(s.on_update, me(Slot::Update));
                }
                Table::ConditionList => add(s.conditions, me(Slot::Conditions)),
                _ => {}
            }
        }
        for c in &self.conditions {
            match table {
                Table::Commands => add(c.on_pass, false),
                Table::ConditionList => add(c.subconditions, false),
                _ => {}
            }
        }
        out
    }

    /// Where a record joining a state's span at `pos` goes, refusing a place
    /// inside a span some other list also covers.
    fn place_in_span(&self, table: Table, state: usize, slot: Slot, pos: usize) -> Result<usize> {
        let span = self.slot(state, slot)?;
        if span.count == 0 {
            return Ok(match table {
                Table::Commands => self.commands.len(),
                _ => self.condition_list.len(),
            });
        }
        let first = self
            .index_at(table, span.offset)
            .ok_or_else(|| format_err("a state's span points outside its table"))?;
        if pos > span.count as usize {
            return Err(format_err(format!(
                "no place {pos} in a span of {}",
                span.count
            )));
        }
        let at = first + pos;
        for (s, c, is_mine) in self.spans_into(table, Some((state, slot))) {
            if !is_mine && s < at && at < s + c {
                return Err(format_err(format!(
                    "state {state}'s {slot:?} shares records with another list"
                )));
            }
        }
        Ok(at)
    }

    fn slot(&self, state: usize, slot: Slot) -> Result<Span> {
        let s = self
            .states
            .get(state)
            .ok_or_else(|| format_err(format!("no state {state}")))?;
        Ok(match slot {
            Slot::Conditions => s.conditions,
            Slot::Enter => s.on_enter,
            Slot::Exit => s.on_exit,
            Slot::Update => s.on_update,
        })
    }

    fn slot_mut(&mut self, state: usize, slot: Slot) -> &mut Span {
        let s = &mut self.states[state];
        match slot {
            Slot::Conditions => &mut s.conditions,
            Slot::Enter => &mut s.on_enter,
            Slot::Exit => &mut s.on_exit,
            Slot::Update => &mut s.on_update,
        }
    }

    /// A new state at the end of machine `group`'s states, with `id`; its index
    /// in [`Esd::states`]. States of later machines move up one.
    pub fn add_state(&mut self, group: usize, id: i64) -> Result<usize> {
        let g = self
            .groups
            .get(group)
            .ok_or_else(|| format_err(format!("no machine {group}")))?;
        let first = self
            .index_at(Table::States, g.states.offset)
            .filter(|_| g.states.count > 0)
            .ok_or_else(|| format_err("a machine with no states cannot take one"))?;
        let at = first + g.states.count as usize;
        if self.states_of(g).iter().any(|s| s.id == id) {
            return Err(format_err(format!("machine already has a state {id}")));
        }
        for (gi, other) in self.groups.iter().enumerate() {
            let Some(s) = self.index_at(Table::States, other.states.offset) else {
                continue;
            };
            if gi != group && s < at && at < s + other.states.count as usize {
                return Err(format_err("machines share states"));
            }
        }
        self.insert(Table::States, at, 1)?;
        self.groups[group].states.count += 1;
        self.states[at].id = id;
        Ok(at)
    }

    /// A command put into a state's commands run on entry, at position `pos`,
    /// with each argument's expression (each ending with [`crate::expr::END`]).
    pub fn insert_command(
        &mut self,
        state: usize,
        pos: usize,
        bank: i32,
        id: i32,
        args: &[Vec<u8>],
    ) -> Result<()> {
        let at = self.place_in_span(Table::Commands, state, Slot::Enter, pos)?;
        let span = self.slot(state, Slot::Enter)?;
        let first = if span.count == 0 {
            at
        } else {
            self.index_at(Table::Commands, span.offset).unwrap_or(at)
        };
        self.insert(Table::Commands, at, 1)?;
        let first_offset = self.offset_of(Table::Commands, first);
        let s = self.slot_mut(state, Slot::Enter);
        s.offset = first_offset;
        s.count += 1;
        let a = self.args.len();
        self.insert(Table::Args, a, args.len())?;
        let args_span = if args.is_empty() {
            NO_SPAN
        } else {
            Span {
                offset: self.offset_of(Table::Args, a),
                count: args.len() as i64,
            }
        };
        self.commands[at] = Command {
            bank,
            id,
            args: args_span,
        };
        for (i, e) in args.iter().enumerate() {
            self.args[a + i] = self.push_expression(e);
        }
        Ok(())
    }

    /// A condition put into a state's list at position `pos`: when `evaluator`
    /// holds, the machine moves to state `to` (an index in [`Esd::states`]).
    pub fn insert_condition(
        &mut self,
        state: usize,
        pos: usize,
        to: Option<usize>,
        evaluator: &[u8],
    ) -> Result<()> {
        if to.is_some_and(|t| t >= self.states.len()) {
            return Err(format_err("a condition moving to no state"));
        }
        let c = self.conditions.len();
        self.insert(Table::Conditions, c, 1)?;
        let at = self.place_in_span(Table::ConditionList, state, Slot::Conditions, pos)?;
        let span = self.slot(state, Slot::Conditions)?;
        let first = if span.count == 0 {
            at
        } else {
            self.index_at(Table::ConditionList, span.offset)
                .unwrap_or(at)
        };
        self.insert(Table::ConditionList, at, 1)?;
        let first_offset = self.offset_of(Table::ConditionList, first);
        let s = self.slot_mut(state, Slot::Conditions);
        s.offset = first_offset;
        s.count += 1;
        self.condition_list[at] = self.offset_of(Table::Conditions, c);
        let next_state = to.map_or(-1, |t| self.offset_of(Table::States, t));
        let evaluator = self.push_expression(evaluator);
        self.conditions[c] = Condition {
            next_state,
            on_pass: NO_SPAN,
            subconditions: NO_SPAN,
            evaluator,
        };
        Ok(())
    }
}

/// Which of a state's lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Conditions,
    Enter,
    Exit,
    Update,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{END, INT};
    use crate::{GROUP_LEN, Group, SCRIPT_LEN};

    /// Two machines of two states each. Machine 0's first state runs one
    /// command, 1[11](7, 1), and moves on to its second when `1` holds; the
    /// second machine's states share one command span, and its first state's
    /// condition moves to its second.
    fn sample() -> Esd {
        let mut esd = Esd {
            unk18: 6,
            hash: [3; 16],
            groups: vec![],
            states: vec![],
            conditions: vec![],
            commands: vec![],
            args: vec![],
            condition_list: vec![],
            expressions: vec![],
            name: "t100001000".encode_utf16().collect(),
            nameless: false,
        };
        let states_at = (SCRIPT_LEN + 2 * GROUP_LEN) as i64;
        let st = |i: i64| states_at + i * STATE_LEN as i64;
        let conds_at = st(4);
        let cmds_at = conds_at + 2 * CONDITION_LEN as i64;
        let args_at = cmds_at + 2 * COMMAND_LEN as i64;
        let list_at = args_at + 3 * ARG_LEN as i64;
        let expr_at = list_at + 2 * 8;
        let span = |offset, count| Span { offset, count };
        esd.groups = vec![
            Group {
                id: 0x7FFF_FFFF,
                states: span(st(0), 2),
                states_again: st(0),
            },
            Group {
                id: 0x7FFF_FFFE,
                states: span(st(2), 2),
                states_again: st(2),
            },
        ];
        esd.states = vec![
            State {
                id: 0,
                conditions: span(list_at, 1),
                on_enter: span(cmds_at, 1),
                on_exit: NO_SPAN,
                on_update: NO_SPAN,
            },
            State {
                id: 1,
                conditions: NO_SPAN,
                on_enter: NO_SPAN,
                on_exit: NO_SPAN,
                on_update: NO_SPAN,
            },
            State {
                id: 0,
                conditions: span(list_at + 8, 1),
                on_enter: span(cmds_at + COMMAND_LEN as i64, 1),
                on_exit: NO_SPAN,
                on_update: NO_SPAN,
            },
            State {
                id: 1,
                conditions: NO_SPAN,
                on_enter: span(cmds_at + COMMAND_LEN as i64, 1),
                on_exit: NO_SPAN,
                on_update: NO_SPAN,
            },
        ];
        // Expressions: "1" (2 bytes), "7" (2), "1" again (2), "100000" (6).
        esd.expressions = vec![
            0x41, END, 0x47, END, 0x41, END, INT, 0xA0, 0x86, 0x01, 0x00, END,
        ];
        esd.conditions = vec![
            Condition {
                next_state: st(1),
                on_pass: NO_SPAN,
                subconditions: NO_SPAN,
                evaluator: span(expr_at, 2),
            },
            Condition {
                next_state: st(3),
                on_pass: NO_SPAN,
                subconditions: NO_SPAN,
                evaluator: span(expr_at, 2),
            },
        ];
        esd.commands = vec![
            Command {
                bank: 1,
                id: 11,
                args: span(args_at, 2),
            },
            Command {
                bank: 1,
                id: 104,
                args: span(args_at + 2 * ARG_LEN as i64, 1),
            },
        ];
        esd.args = vec![
            span(expr_at + 2, 2),
            span(expr_at + 4, 2),
            span(expr_at + 6, 6),
        ];
        esd.condition_list = vec![conds_at, conds_at + CONDITION_LEN as i64];
        esd
    }

    /// What a script says, with every offset read as what it points at: each
    /// machine's states, each with its commands (bank, id, argument bytes) and
    /// conditions (evaluator bytes, the state moved to by machine and place).
    fn meaning(esd: &Esd) -> Vec<Vec<String>> {
        let place = |offset: i64| {
            let (i, _) = esd.state_at(offset)?;
            esd.groups.iter().enumerate().find_map(|(gi, g)| {
                let first = esd.index_at(Table::States, g.states.offset)?;
                (first..first + g.states.count as usize)
                    .contains(&i)
                    .then_some((gi, i - first))
            })
        };
        esd.groups
            .iter()
            .map(|g| {
                esd.states_of(g)
                    .iter()
                    .map(|s| {
                        let cmds: Vec<String> = [s.on_enter, s.on_exit, s.on_update]
                            .iter()
                            .map(|sp| {
                                esd.commands_in(*sp)
                                    .iter()
                                    .map(|c| {
                                        format!("{}[{}]{:?}", c.bank, c.id, esd.command_args(c))
                                    })
                                    .collect::<Vec<_>>()
                                    .join(";")
                            })
                            .collect();
                        let conds: Vec<String> = esd
                            .conditions_in(s.conditions)
                            .iter()
                            .map(|c| {
                                format!(
                                    "{:?}->{:?}",
                                    esd.expression(c.evaluator),
                                    place(c.next_state)
                                )
                            })
                            .collect();
                        format!("{} {cmds:?} {conds:?}", s.id)
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn the_sample_reads_back_and_means_what_it_says() {
        let esd = sample();
        let back = Esd::read(&esd.write().unwrap()).unwrap();
        assert_eq!(back, esd);
        let m = meaning(&esd);
        assert!(
            m[0][0].contains("1[11][Some([71, 161]), Some([65, 161])]"),
            "{m:?}"
        );
        assert!(m[0][0].ends_with("->Some((0, 1))\"]"), "{m:?}");
        assert!(m[1][0].ends_with("->Some((1, 1))\"]"), "{m:?}");
    }

    #[test]
    fn nothing_put_in_changes_nothing() {
        let mut esd = sample();
        let before = esd.write().unwrap();
        for t in Table::ALL {
            esd.insert(t, 0, 0).unwrap();
        }
        assert_eq!(esd.write().unwrap(), before);
    }

    #[test]
    fn records_put_anywhere_leave_the_script_meaning_the_same() {
        let base = sample();
        let want = meaning(&base);
        let lens = [
            base.states.len(),
            base.conditions.len(),
            base.commands.len(),
            base.args.len(),
            base.condition_list.len(),
        ];
        for t in Table::ALL {
            if t == Table::States {
                continue;
            }
            for at in 0..=lens[t.index()] {
                // A record inside a span joins it; only places between spans keep the meaning.
                let mut spans = base.spans_into(t, None);
                if t == Table::Args {
                    spans = base
                        .commands
                        .iter()
                        .filter_map(|c| {
                            Some((
                                base.index_at(t, c.args.offset)?,
                                c.args.count as usize,
                                false,
                            ))
                        })
                        .collect();
                }
                if spans.iter().any(|&(s, c, _)| s < at && at < s + c) {
                    continue;
                }
                let mut esd = base.clone();
                esd.insert(t, at, 2).unwrap();
                let back = Esd::read(&esd.write().unwrap()).unwrap();
                assert_eq!(meaning(&back), want, "{t:?} at {at}");
            }
        }
    }

    #[test]
    fn a_state_added_to_the_first_machine_moves_the_second_machines_states() {
        let mut esd = sample();
        let i = esd.add_state(0, 7).unwrap();
        assert_eq!(i, 2);
        let back = Esd::read(&esd.write().unwrap()).unwrap();
        let m = meaning(&back);
        assert_eq!(m[0].len(), 3);
        assert!(m[0][2].starts_with("7 "), "{m:?}");
        assert_eq!(
            m[1],
            meaning(&sample())[1],
            "the second machine is unchanged"
        );
        assert!(esd.add_state(0, 7).is_err(), "an id twice");
    }

    #[test]
    fn a_command_and_a_condition_go_where_they_are_put() {
        let mut esd = sample();
        let new = esd.add_state(0, 9).unwrap();
        esd.insert_command(new, 0, 1, 104, &[vec![0x48, END]])
            .unwrap();
        esd.insert_command(new, 1, 1, 11, &[vec![0x49, END], vec![0x41, END]])
            .unwrap();
        esd.insert_condition(new, 0, Some(0), &[0x41, END]).unwrap();
        // Before the first state's own condition, and its command after the first.
        esd.insert_condition(0, 0, Some(new), &[0x40, END]).unwrap();
        esd.insert_command(0, 1, 1, 20, &[]).unwrap();
        let back = Esd::read(&esd.write().unwrap()).unwrap();
        assert_eq!(back, esd);
        let m = meaning(&back);
        assert!(
            m[0][2].starts_with(
                "9 [\"1[104][Some([72, 161])];1[11][Some([73, 161]), Some([65, 161])]\", \"\", \"\"]"
            ),
            "{m:?}"
        );
        assert!(
            m[0][2].ends_with("[\"Some([65, 161])->Some((0, 0))\"]"),
            "{m:?}"
        );
        assert!(
            m[0][0].contains("1[11][Some([71, 161]), Some([65, 161])];1[20][]"),
            "{m:?}"
        );
        assert!(
            m[0][0].ends_with(
                "[\"Some([64, 161])->Some((0, 2))\", \"Some([65, 161])->Some((0, 1))\"]"
            ),
            "{m:?}"
        );
        assert_eq!(
            m[1],
            meaning(&sample())[1],
            "the second machine is unchanged"
        );
    }

    #[test]
    fn a_shared_span_is_changed_for_one_of_its_states_only() {
        // The second machine's two states run the same one command: one more
        // before it or after it reaches only the state given.
        for pos in [0, 1] {
            let mut esd = sample();
            esd.insert_command(2, pos, 1, 20, &[]).unwrap();
            let back = Esd::read(&esd.write().unwrap()).unwrap();
            let m = meaning(&back);
            assert!(
                m[1][0].contains("1[104]") && m[1][0].contains("1[20]"),
                "{m:?}"
            );
            assert!(
                m[1][1].contains("1[104]") && !m[1][1].contains("1[20]"),
                "{m:?}"
            );
        }
    }

    #[test]
    fn a_record_inside_another_states_span_is_refused() {
        // State 2 runs both commands, state 3 only the first: one more after
        // state 3's would land inside state 2's.
        let mut esd = sample();
        esd.states[2].on_enter = Span {
            offset: esd.offset_of(Table::Commands, 0),
            count: 2,
        };
        esd.states[3].on_enter = Span {
            offset: esd.offset_of(Table::Commands, 0),
            count: 1,
        };
        assert!(esd.insert_command(3, 1, 1, 20, &[]).is_err());
        assert!(esd.insert_command(2, 2, 1, 20, &[]).is_ok());
    }
}
