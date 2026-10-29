//! EMEVD: the game's event scripts. Each event is a list of instructions
//! (a bank and an id, with an argument blob), and parameters that copy an
//! event's own arguments into its instructions when the event is started.
//!
//! Observed on Elden Ring 1.17.2's `event/*.emevd.dcx` (little-endian, 64-bit):
//!
//! ```text
//! 0x00 "EVD\0"  0x04 u8 0 (little-endian)  0x05 u8 0xFF (64-bit)  0x06 u8 1
//! 0x07 u8 0xFF  0x08 u32 0xCD (version)    0x0C u32 file size
//! 0x10 then eight (i64 count, i64 offset) pairs: events, instructions, a
//!      section always empty, layers, parameters, linked files; then
//!      (i64 length, i64 offset) for the argument blob and for strings.
//! 0x90 events (0x30): i64 id, i64 instruction count, i64 offset into the
//!      instructions, i64 parameter count, i64 offset into the parameters
//!      (-1 when none), u8 rest behaviour, 7 zeros
//!      instructions (0x20): u32 bank, u32 id, i64 argument length,
//!      i64 offset into the blob (when there are none, 0 in 264 of the game's
//!      files and -1 in the other 325, one or the other throughout a file),
//!      i64 offset into the layers (-1 when none)
//!      layers (0x20 each, kept as read)
//!      argument blob, padded with zeros to 16
//!      parameters (0x20): i64 instruction index, i64 target byte,
//!      i64 source byte, i32 byte count, i32 (kept as read)
//!      linked files: i64 offsets into the strings
//!      strings: UTF-16 names of linked files, with their padding, kept as read
//! ```
//!
//! The writer lays everything out again from the events, and `apoc-fs check`
//! holds every EMEVD in the game to coming back byte for byte.

use fs_bytes::{self as rd, Error, Result, format_err};

const HEADER_LEN: usize = 0x90;
const EVENT_LEN: usize = 0x30;
const INSTRUCTION_LEN: usize = 0x20;
const LAYER_LEN: usize = 0x20;
const PARAMETER_LEN: usize = 0x20;
const MAGIC: &[u8; 8] = b"EVD\0\0\xff\x01\xff";
const VERSION: u32 = 0xCD;

/// One instruction: what to do (bank and id) and with what (arguments).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    pub bank: u32,
    pub id: u32,
    pub args: Vec<u8>,
    /// The instruction's layer record, kept as read.
    pub layer: Option<[u8; LAYER_LEN]>,
}

/// Copies `size` bytes of the event's own arguments, from `source`, into
/// instruction `instruction`'s arguments at `target`, when the event starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parameter {
    pub instruction: i64,
    pub target: i64,
    pub source: i64,
    pub size: i32,
    pub unk: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub id: i64,
    /// What happens to the event when the player rests: 0 or 1 in the game's
    /// files.
    pub rest: u8,
    pub instructions: Vec<Instruction>,
    pub parameters: Vec<Parameter>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Emevd {
    pub events: Vec<Event>,
    /// Offsets into `strings` of the files this one links (e.g.
    /// `common_func.emevd`).
    pub linked_files: Vec<i64>,
    /// The string area as read: UTF-16 names and their padding.
    pub strings: Vec<u8>,
    /// Where an instruction with no arguments points: 0 or -1, depending on
    /// the file.
    pub empty_args_offset: i64,
}

/// Whether `b` starts like an EMEVD.
pub fn is_emevd(b: &[u8]) -> bool {
    b.starts_with(b"EVD\0")
}

fn section(b: &[u8], at: usize, len: usize, what: &'static str) -> Result<(usize, usize)> {
    let count = rd::index(rd::i64_le(b, at, "EMEVD header")?, what)?;
    let offset = rd::index(rd::i64_le(b, at + 8, "EMEVD header")?, what)?;
    if count
        .checked_mul(len)
        .is_none_or(|n| offset.saturating_add(n) > b.len())
    {
        return Err(format_err(format!("the EMEVD's {what} do not fit it")));
    }
    Ok((count, offset))
}

impl Emevd {
    pub fn read(b: &[u8]) -> Result<Self> {
        if rd::bytes(b, 0, 8, "EMEVD header")? != MAGIC
            || rd::u32_le(b, 8, "EMEVD header")? != VERSION
        {
            return Err(Error::Unsupported(
                "this EMEVD is not a little-endian 64-bit version 0xCD script".into(),
            ));
        }
        if rd::u32_le(b, 0x0C, "EMEVD header")? as usize != b.len() {
            return Err(format_err(
                "the EMEVD's size field does not match its length",
            ));
        }
        let (n_events, events_at) = section(b, 0x10, EVENT_LEN, "events")?;
        let (n_instr, instr_at) = section(b, 0x20, INSTRUCTION_LEN, "instructions")?;
        if rd::i64_le(b, 0x30, "EMEVD header")? != 0 {
            return Err(Error::Unsupported(
                "this EMEVD uses a section never seen in the game's files".into(),
            ));
        }
        let (n_layers, layers_at) = section(b, 0x40, LAYER_LEN, "layers")?;
        let (n_params, params_at) = section(b, 0x50, PARAMETER_LEN, "parameters")?;
        let (n_files, files_at) = section(b, 0x60, 8, "linked files")?;
        let (args_len, args_at) = section(b, 0x70, 1, "argument blob")?;
        let (strings_len, strings_at) = section(b, 0x80, 1, "strings")?;
        let args = &b[args_at..args_at + args_len];
        let mut empty_args_offset = None;

        let mut events = Vec::with_capacity(n_events);
        for e in 0..n_events {
            let at = events_at + e * EVENT_LEN;
            let id = rd::i64_le(b, at, "event")?;
            let count = rd::index(rd::i64_le(b, at + 8, "event")?, "instruction count")?;
            let first = rd::index(rd::i64_le(b, at + 0x10, "event")?, "instruction offset")?;
            let p_count = rd::index(rd::i64_le(b, at + 0x18, "event")?, "parameter count")?;
            let p_first = rd::i64_le(b, at + 0x20, "event")?;
            let rest = rd::u8_at(b, at + 0x28, "event")?;
            if rd::bytes(b, at + 0x29, 7, "event")? != [0; 7] {
                return Err(Error::Unsupported(
                    "an EMEVD event has unknown values in its padding".into(),
                ));
            }
            if !first.is_multiple_of(INSTRUCTION_LEN)
                || (first / INSTRUCTION_LEN)
                    .checked_add(count)
                    .is_none_or(|end| end > n_instr)
            {
                return Err(format_err(format!(
                    "event {id}'s instructions run past the table"
                )));
            }
            let mut instructions = Vec::with_capacity(count);
            for i in 0..count {
                let at = instr_at + first + i * INSTRUCTION_LEN;
                let bank = rd::u32_le(b, at, "instruction")?;
                let iid = rd::u32_le(b, at + 4, "instruction")?;
                let len = rd::index(rd::i64_le(b, at + 8, "instruction")?, "argument length")?;
                let raw_off = rd::i64_le(b, at + 0x10, "instruction")?;
                let layer_at = rd::i64_le(b, at + 0x18, "instruction")?;
                let arg = if len == 0 {
                    if *empty_args_offset.get_or_insert(raw_off) != raw_off
                        || !(raw_off == 0 || raw_off == -1)
                    {
                        return Err(Error::Unsupported(
                            "this EMEVD points instructions with no arguments at more than one place".into(),
                        ));
                    }
                    Vec::new()
                } else {
                    let off = rd::index(raw_off, "argument offset")?;
                    rd::bytes(args, off, len, "instruction arguments")?.to_vec()
                };
                let layer = match layer_at {
                    -1 => None,
                    l => {
                        let l = rd::index(l, "layer offset")?;
                        if !l.is_multiple_of(LAYER_LEN) || l / LAYER_LEN >= n_layers {
                            return Err(format_err(
                                "an instruction's layer is outside the layer table",
                            ));
                        }
                        Some(
                            b[layers_at + l..layers_at + l + LAYER_LEN]
                                .try_into()
                                .unwrap(),
                        )
                    }
                };
                instructions.push(Instruction {
                    bank,
                    id: iid,
                    args: arg,
                    layer,
                });
            }
            let parameters = if p_count == 0 {
                if p_first != -1 {
                    return Err(Error::Unsupported(
                        "an event without parameters points at some".into(),
                    ));
                }
                Vec::new()
            } else {
                let p_first = rd::index(p_first, "parameter offset")?;
                if !p_first.is_multiple_of(PARAMETER_LEN)
                    || (p_first / PARAMETER_LEN)
                        .checked_add(p_count)
                        .is_none_or(|end| end > n_params)
                {
                    return Err(format_err(format!(
                        "event {id}'s parameters run past the table"
                    )));
                }
                (0..p_count)
                    .map(|p| {
                        let at = params_at + p_first + p * PARAMETER_LEN;
                        Ok(Parameter {
                            instruction: rd::i64_le(b, at, "parameter")?,
                            target: rd::i64_le(b, at + 8, "parameter")?,
                            source: rd::i64_le(b, at + 0x10, "parameter")?,
                            size: rd::i32_le(b, at + 0x18, "parameter")?,
                            unk: rd::i32_le(b, at + 0x1C, "parameter")?,
                        })
                    })
                    .collect::<Result<_>>()?
            };
            events.push(Event {
                id,
                rest,
                instructions,
                parameters,
            });
        }
        let linked_files = (0..n_files)
            .map(|i| rd::i64_le(b, files_at + i * 8, "linked file"))
            .collect::<Result<_>>()?;
        Ok(Emevd {
            events,
            linked_files,
            strings: b[strings_at..strings_at + strings_len].to_vec(),
            empty_args_offset: empty_args_offset.unwrap_or(0),
        })
    }

    pub fn write(&self) -> Result<Vec<u8>> {
        let n_instr: usize = self.events.iter().map(|e| e.instructions.len()).sum();
        let n_params: usize = self.events.iter().map(|e| e.parameters.len()).sum();
        let n_layers = self
            .events
            .iter()
            .flat_map(|e| &e.instructions)
            .filter(|i| i.layer.is_some())
            .count();
        let events_at = HEADER_LEN;
        let instr_at = events_at + self.events.len() * EVENT_LEN;
        let layers_at = instr_at + n_instr * INSTRUCTION_LEN;
        let args_at = layers_at + n_layers * LAYER_LEN;

        let mut out = vec![0u8; args_at];
        let mut args: Vec<u8> = Vec::new();
        let (mut instr_i, mut layer_i, mut param_i) = (0usize, 0usize, 0usize);
        for (e, ev) in self.events.iter().enumerate() {
            let at = events_at + e * EVENT_LEN;
            put(&mut out, at, ev.id);
            put(&mut out, at + 8, ev.instructions.len() as i64);
            put(&mut out, at + 0x10, (instr_i * INSTRUCTION_LEN) as i64);
            put(&mut out, at + 0x18, ev.parameters.len() as i64);
            put(
                &mut out,
                at + 0x20,
                if ev.parameters.is_empty() {
                    -1
                } else {
                    (param_i * PARAMETER_LEN) as i64
                },
            );
            out[at + 0x28] = ev.rest;
            for ins in &ev.instructions {
                if !ins.args.len().is_multiple_of(4) {
                    return Err(format_err(
                        "instruction arguments must be a multiple of 4 bytes",
                    ));
                }
                let at = instr_at + instr_i * INSTRUCTION_LEN;
                out[at..at + 4].copy_from_slice(&ins.bank.to_le_bytes());
                out[at + 4..at + 8].copy_from_slice(&ins.id.to_le_bytes());
                put(&mut out, at + 8, ins.args.len() as i64);
                put(
                    &mut out,
                    at + 0x10,
                    if ins.args.is_empty() {
                        self.empty_args_offset
                    } else {
                        args.len() as i64
                    },
                );
                args.extend_from_slice(&ins.args);
                match &ins.layer {
                    None => put(&mut out, at + 0x18, -1),
                    Some(layer) => {
                        let l = layers_at + layer_i * LAYER_LEN;
                        out[l..l + LAYER_LEN].copy_from_slice(layer);
                        put(&mut out, at + 0x18, (layer_i * LAYER_LEN) as i64);
                        layer_i += 1;
                    }
                }
                instr_i += 1;
            }
            param_i += ev.parameters.len();
        }
        args.resize(fs_bytes::align_up(args.len(), 16), 0);
        out.extend_from_slice(&args);
        let params_at = out.len();
        for p in self.events.iter().flat_map(|e| &e.parameters) {
            out.extend_from_slice(&p.instruction.to_le_bytes());
            out.extend_from_slice(&p.target.to_le_bytes());
            out.extend_from_slice(&p.source.to_le_bytes());
            out.extend_from_slice(&p.size.to_le_bytes());
            out.extend_from_slice(&p.unk.to_le_bytes());
        }
        let files_at = out.len();
        for f in &self.linked_files {
            out.extend_from_slice(&f.to_le_bytes());
        }
        let strings_at = out.len();
        out.extend_from_slice(&self.strings);

        out[0..8].copy_from_slice(MAGIC);
        out[8..12].copy_from_slice(&VERSION.to_le_bytes());
        let size = u32::try_from(out.len()).map_err(|_| format_err("the EMEVD is too large"))?;
        out[0x0C..0x10].copy_from_slice(&size.to_le_bytes());
        for (at, count, offset) in [
            (0x10, self.events.len(), events_at),
            (0x20, n_instr, instr_at),
            (0x30, 0, layers_at),
            (0x40, n_layers, layers_at),
            (0x50, n_params, params_at),
            (0x60, self.linked_files.len(), files_at),
            (0x70, args.len(), args_at),
            (0x80, self.strings.len(), strings_at),
        ] {
            put(&mut out, at, count as i64);
            put(&mut out, at + 8, offset as i64);
        }
        Ok(out)
    }
}

fn put(out: &mut [u8], at: usize, v: i64) {
    out[at..at + 8].copy_from_slice(&v.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ins(bank: u32, id: u32, args: &[u32]) -> Instruction {
        Instruction {
            bank,
            id,
            args: args.iter().flat_map(|a| a.to_le_bytes()).collect(),
            layer: None,
        }
    }

    fn sample() -> Emevd {
        let name: Vec<u8> = "N:\\common_func.emevd\0"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        let mut strings = name.clone();
        strings.resize(64, 0);
        Emevd {
            events: vec![
                Event {
                    id: 0,
                    rest: 0,
                    instructions: vec![ins(2000, 0, &[1, 2, 3]), ins(1000, 3, &[])],
                    parameters: vec![],
                },
                Event {
                    id: 10002281,
                    rest: 1,
                    instructions: vec![ins(2009, 3, &[0, 0]), ins(1003, 2, &[7])],
                    parameters: vec![Parameter {
                        instruction: 1,
                        target: 0,
                        source: 4,
                        size: 4,
                        unk: 2,
                    }],
                },
            ],
            linked_files: vec![0],
            strings,
            empty_args_offset: 0,
        }
    }

    #[test]
    fn a_script_reads_back_what_was_written_and_a_second_write_is_identical() {
        let bytes = sample().write().unwrap();
        let back = Emevd::read(&bytes).unwrap();
        assert_eq!(back, sample());
        assert_eq!(back.write().unwrap(), bytes);
        assert_eq!(
            u32::from_le_bytes(bytes[0x0C..0x10].try_into().unwrap()) as usize,
            bytes.len()
        );
    }

    #[test]
    fn instructions_with_no_arguments_point_at_zero_and_the_blob_is_padded() {
        let bytes = sample().write().unwrap();
        let instr_at = u64::from_le_bytes(bytes[0x28..0x30].try_into().unwrap()) as usize;
        let second = instr_at + INSTRUCTION_LEN;
        assert_eq!(&bytes[second + 0x10..second + 0x18], &0i64.to_le_bytes());
        let args_len = u64::from_le_bytes(bytes[0x70..0x78].try_into().unwrap());
        assert_eq!(args_len % 16, 0);
    }

    #[test]
    fn either_way_of_marking_no_arguments_is_kept() {
        let mut e = sample();
        e.empty_args_offset = -1;
        let bytes = e.write().unwrap();
        assert_eq!(Emevd::read(&bytes).unwrap(), e);
    }

    #[test]
    fn layers_are_kept() {
        let mut e = sample();
        let mut layer = [0u8; LAYER_LEN];
        layer[0] = 2;
        layer[4] = 0x0F;
        e.events[1].instructions[0].layer = Some(layer);
        let bytes = e.write().unwrap();
        assert_eq!(Emevd::read(&bytes).unwrap(), e);
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
            let _ = Emevd::read(&b);
        }
    }
}
