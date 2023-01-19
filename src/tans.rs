// Table-based assymetric numeral system compression.
//
// Copyright 2021 Robbert Haarman
//
// SPDX-License-Identifier: MIT

use crate::io::{ReadBits, WriteBits};
use crate::result::BoxResult;

pub struct Decoder<'a, S> {
    /// Lookup table. Each entry is (symbol, nbits, base).
    table: &'a [(S, u8, u32)],

    /// Current state.
    state: &'a (S, u8, u32),
}

impl<S: Copy> Decoder<'_, S> {
    pub fn decode_first(&mut self, input: &mut dyn ReadBits) -> BoxResult<S> {
        // We need to read enough bits to set the initial state.
        // In order to be able to encode all the states, we need
        // log2(nstates) bits. The following computes that number.
        let sbits = 32 - ((self.table.len() - 1) as u32).leading_zeros();
        let s = input.read_bits(sbits)?;
        self.state = &self.table[s as usize];
        Ok(self.state.0)
    }
    
    pub fn decode_sym(&mut self, input: &mut dyn ReadBits) -> BoxResult<S> {
        let (_, nbits, base) = self.state;
        let s = *base | input.read_bits(*nbits as u32)?;
        self.state = &self.table[s as usize];
        Ok(self.state.0)
    }
}

pub struct Encoder {
    /// One entry per symbol. First item is coded_nbits, second is an offset
    /// used to get an index into the origins table.
    symtab: Vec::<(u32, u32)>,

    /// Lookup table used to find origin state.
    origin: Vec::<u32>,

    /// Accummulated output
    output: Vec::<u32>,

    /// Accumulated bits not yet stored in output.
    bits: u32,

    /// Current state.
    state: u32,

    /// Total number of states.
    nstates: u32,

    /// Number of bits we need to add to bits before we have 32.
    need_bits: u32,
}

impl Encoder {
    pub fn new(sbits: u32, freqs: &[u32]) -> Encoder {
        let nstates = 1 << sbits;
	let nsyms = freqs.len();
        let mask = nstates - 1;
        let mut encoder = Encoder {
            symtab: Vec::with_capacity(nsyms),
            origin: Vec::new(),
            output: Vec::new(),
            bits: 0,
            state: 0,
            nstates: nstates,
            need_bits: 32,
        };
        encoder.origin.resize(nstates as usize, 0);
        let mut o : u32 = 0;
	
        // Populate symbol table with coded_nbits and offset.
        for s in 0..nsyms {
            let coded_nbits = compute_coded_nbits(freqs[s], sbits);
            let offset = o.wrapping_sub(freqs[s]) & mask;
            encoder.symtab.push((coded_nbits, offset));
            o += freqs[s];
        }
	
        // Populate origin table.
        let stride = compute_stride(nstates);
        let mut o = 0;               // index into origin table
        let mut s = stride & mask;   // state number
        for sym in 0..nsyms {
            for _ in 0..freqs[sym] {
                encoder.origin[o] = s;
                o += 1;
                s = (s + stride) & mask;
            }
        }
        encoder
    }

    fn acc_bits(&mut self, bits: u32, nbits: u32) {
        let mut nbits = nbits;
        while nbits > 0 {
            if self.need_bits > nbits {
                // In self.bits, we accummulate from msb to lsb, so
                // shift bits left as needed.
                self.bits |= (bits & ((1 << nbits) - 1)) << (self.need_bits - nbits);
                self.need_bits -= nbits;
                return;
            }
            self.bits |= bits >> (nbits - self.need_bits);
            self.output.push(self.bits);
            nbits -= self.need_bits;
            self.bits = 0;
            self.need_bits = 32;
        }
    }

    pub fn encode_first(&mut self, sym: u32) {
        // Set encoder state to a state that codes for sym.
        let (coded_nbits, offset) = self.symtab[sym as usize];
        let nbits = coded_nbits >> 24;
        let idx = (self.nstates >> nbits) + offset;
        self.state = self.origin[(idx & (self.nstates - 1)) as usize];
    }

    pub fn encode_sym(&mut self, sym: u32) {
        // Get nbits and offset.
        let (coded_nbits, offset) : (u32, u32) = self.symtab[sym as usize];
        let nbits = (self.state + coded_nbits) >> 24;
        self.acc_bits(self.state, nbits);
        // Set new state.
        let idx = ((self.state + self.nstates) >> nbits) + offset;
        self.state = self.origin[(idx & (self.nstates - 1)) as usize];
    }

    pub fn write(&mut self, output: &mut dyn WriteBits) -> BoxResult<()> {
        let sbits = 32 - (self.nstates - 1).leading_zeros();
        self.acc_bits(self.state, sbits);
        if self.need_bits < 32 {
            output.write_bits(self.bits >> self.need_bits, 32 - self.need_bits as u8)?;
        }
        for i in (0..self.output.len()).rev() {
            output.write_bits(self.output[i], 32)?;
        }
        Ok(())
    }
}

/// Computes a value x such that (x + s) >> 24 gives the number
/// of bits to read in state s.
fn compute_coded_nbits(freq: u32, sbits: u32) -> u32 {
    // For a symbol with no occurrences, return 0.
    if freq == 0 { return 0 }

    // We need sbits bits to encode every possible state value.
    // Not all of these need to be read/written at every state
    // transition: If a symbol has 2**n occurrences in the table,
    // we can get n bits of information from knowing which of
    // the states for that symbol we are in, and only need to
    // encode sbits - n bits.
    //
    // We compute n as 32 - (freq - 1).leading_zeros(), which
    // gives us a lower bound on the number of bits that need to
    // be encode for the state transition.
    let low_nbits = sbits - (32 - (freq - 1).leading_zeros());

    // Number of successor states we can get to using freq
    // origin states and low_nbits per state.
    let covered = freq << low_nbits;

    // If we do not already cover all possible successor states,
    // we will increase our coverage by adding an extra bit to
    // encode for some states. We do this for successor states
    // starting at some threshold. Since we have 1 << sbits
    // successor states, we need to cover an additional
    // (1 << sbits) - covered states. The threshold, then, is
    // the old value of covered minus the additional states to be
    // covered, which can be simplified to:
    let threshold = covered + covered - (1 << sbits);

    // The return value is computed so that adding the number
    // of the successor state, then right-shifting the result by
    // 24 results in the number of bits to encode for the state.
    ((low_nbits + 1) << 24) - threshold
}


/// Compute stride so that:
/// (a) It is a relative prime of nstates.
/// (b) It is a bit over half of nstates.
/// Property (a) ensures that a single iteration will populate all states.
/// Property (b) ensures that symbols with multiple occurrences will
/// be spread roughly evenly across the state space.
fn compute_stride(nstates: u32) -> u32 {
  if nstates <= 8 {
    return 5;
  } else {
    return (nstates >> 1) + (nstates >> 3) + 3;
  }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::io::{BitReader, BitWriter};

    const EXAMPLE_TABLE : &[(char, u8, u32)] = &[
        ('c', 3, 0),    // 0
        ('b', 1, 6),    // 1
        ('a', 2, 4),    // 2
        ('b', 0, 1),    // 3
        ('b', 1, 4),    // 4
        ('a', 2, 0),    // 5
        ('b', 0, 0),    // 6
        ('b', 1, 2),    // 7
    ];

    fn tans_prev_state<S: Copy + Eq>(successor: u32,
                                     sym: S,
                                     table: &[(S, u8, u32)]) -> u32 {
        for i in 0..table.len() {
            let (s, nbits, base) = table[i];
            if s == sym && base <= successor && (base + (1 << nbits)) > successor {
                return i as u32;
            }
        }
        panic!("Cannot encode symbol; table malformed.");
    }

    #[test]
    fn test_coded_nbits() {
        // 8 total states, bits for a state that occurs 4 times should be 1 for every successor state.
        let coded_one = compute_coded_nbits(4, 3);
        assert_eq!(coded_one >> 24, 1);
        assert_eq!((coded_one + 7) >> 24, 1);

        // 8 total states, bits for a state that occurs 3 times should be 1 for every successor
        // state < 4, and 2 for successor state >= 4.
        let coded_one_two = compute_coded_nbits(3, 3);
        assert_eq!(coded_one_two >> 24, 1);
        assert_eq!((coded_one_two + 3) >> 24, 1);
        assert_eq!((coded_one_two + 4) >> 24, 2);
        assert_eq!((coded_one_two + 7) >> 24, 2);
    }
    
    #[test]
    fn decode_abbc() {
        let mut input = &[0xd][..];
        let mut reader = BitReader::new(&mut input);
        let mut decoder = Decoder {
            table: EXAMPLE_TABLE,
            state: &EXAMPLE_TABLE[0],
        };
        assert_eq!(decoder.decode_first(&mut reader).unwrap(), 'a');
        assert_eq!(decoder.decode_sym(&mut reader).unwrap(), 'b');
        assert_eq!(decoder.decode_sym(&mut reader).unwrap(), 'b');
        assert_eq!(decoder.decode_sym(&mut reader).unwrap(), 'c');
    }

    #[test]
    fn encode_abbac_slow() {
        let table = EXAMPLE_TABLE;
        let s = tans_prev_state(0, 'c', table);
        assert_eq!(s, 0);
        let s = tans_prev_state(s, 'a', table);
        assert_eq!(s, 5);
        let s = tans_prev_state(s, 'b', table);
        assert_eq!(s, 4);
        let s = tans_prev_state(s, 'b', table);
        assert_eq!(s, 4);
        let s = tans_prev_state(s, 'a', table);
        assert_eq!(s, 2);
    }

    #[test]
    fn encoder_new() {
        let freqs = &[2, 5, 1];
        let encoder = Encoder::new(3, freqs);
        assert_eq!((encoder.symtab[0].0 + 7) >> 24, 2);
        assert_eq!(encoder.symtab[0].1, 6);
        assert_eq!((encoder.symtab[1].0 + 1) >> 24, 0);
        assert_eq!((encoder.symtab[1].0 + 7) >> 24, 1);
        assert_eq!(encoder.symtab[1].1, 5);
        assert_eq!((encoder.symtab[2].0 + 7) >> 24, 3);
        assert_eq!(encoder.symtab[2].1, 6);
        assert_eq!(encoder.origin[0], 5);
        assert_eq!(encoder.origin[1], 2);
        assert_eq!(encoder.origin[2], 7);
    }

    #[test]
    fn encode_abbac() {
        let freqs = &[2, 5, 1];
        let mut output = Vec::new();
        let mut writer = BitWriter::new(&mut output);
        let mut encoder = Encoder::new(3, freqs);
        // The state table should be
        //   state | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
        //     sym | c | b | a | b | b | a | b | b |
        //   nbits | 3 | 1 | 2 | 0 | 1 | 2 | 0 | 1 |
        //    base | 0 | 6 | 4 | 1 | 4 | 0 | 0 | 2 |
        encoder.encode_first(2);
        assert_eq!(encoder.state, 0);
        encoder.encode_sym(0);
        assert_eq!(encoder.state, 5);
        encoder.encode_sym(1);
        assert_eq!(encoder.state, 4);
        encoder.encode_sym(1);
        assert_eq!(encoder.state, 4);
        encoder.encode_sym(0);
        assert_eq!(encoder.state, 2);
        assert!(encoder.write(&mut writer).is_ok());
        assert!(writer.flush().is_ok());
        assert_eq!(output, b"\x42\x00");
    }
}
