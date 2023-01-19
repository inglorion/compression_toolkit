// Simple compression using backreferences.
//
// Copyright 2021 Robbert Haarman
//
// SPDX-License-Identifier: MIT
//
// File format: A compressed stream consists of zero or more runs.
// Each run starts with a byte indicating the type of run and its
// length in bytes. There are two types of runs: literal runs
// and backreferences. Literal runs have the most significant bit
// of the lead byte set to 0, whereas backreferences have the most
// significant bit set to 1.
//
// Literal run format:
//
//   +--------+-------------------------+
//   |0xxxxxxx| xxxxxxx bytes           |
//   +--------+-------------------------+
//
// Backreference format:
//
//   +--------+--------+--------+
//   |1xxxxxxx|dddddddd|dddddddd|
//   +--------+--------+--------+
//
// A literal run is decoded by copying the bytes after the lead byte
// to the output. For example, the bytes
//
//   3, 120, 121, 123
//
// Result in the output "xyz".
//
// A backreference is decoded by reading the two bytes that indicate
// the distance, interpreting the first byte as the 8 least significant
// bits and the second byte as the 8 most significant bits. Then, bytes
// starting (distance + 3) bytes before the current position are copied
// to the output. It is possible for the length of a backreference to
// be greater than the distance.
//
// For example, the following bytes:
//
//   134, 1, 0
//
// Result in copying 6 (134 - 128) bytes, starting from 4 (1 + 3) bytes
// ago. If the last 4 bytes we had decoded were "1234", then the
// text resulting from the backreference would be "123412".

use crate::io::{IOTrait, LookbackInput, RepeatOutput};
use crate::result::BoxResult;

pub fn decode<IO: IOTrait + RepeatOutput>(io: &mut IO) -> BoxResult<()> {
    while let Some(b) = io.next_byte()? {
        if b < 128 {
            io.copy_bytes(b as usize)?;
        } else {
            let lo = io.next_byte()?.expect("end of input inside backreference");
            let hi = io.next_byte()?.expect("end of input inside backreference");
            let dist = ((hi as usize) << 8) | lo as usize;
            io.repeat_bytes((b & 0x7f) as usize, dist)?;
        }
    }
    Ok(())
}

struct EncoderState {
    // We find repititions by computing a rolling hash of the most recently
    // seen 3 bytes.
    hash: u32,
    
    // We limit the magnitude of hash by bitwise anding with this mask.
    hash_mask: u32,
    
    // For each hash value, we record the most recent position at which
    // we have encountered it.

    pos: std::vec::Vec::<u64>,
    // The literal length we have accumulated so far.
    litlen: u8,
}

impl EncoderState {
    fn new() -> EncoderState {
        let mask = (1 << 14) - 1;
        let mut pos_table = Vec::new();
        pos_table.resize(mask + 1, 0);
        EncoderState {
            hash: 0,
            hash_mask: mask as u32,
            pos: pos_table,
            litlen: 0,
        }
    }
    
    fn find_rep<IO: IOTrait + LookbackInput>(&mut self, io: &mut IO)
                                             -> BoxResult<(u8, u8, u64)> {
        while let Some(b) = io.next_byte()? {
            let pos = io.inpos();
            let prev = self.update_hash(b, pos);
            // Only return matches of length at least 3 that occur within
            // 0x10000 bytes of the current position.
            if prev >= 3 && (pos < 0x10000 || prev > pos - 0x10000) &&
                io.lookback(prev - 3) == io.lookback(pos - 3) &&
                io.lookback(prev - 2) == io.lookback(pos - 2) &&
                io.lookback(prev - 1) == io.lookback(pos - 1)
            {
                return self.found_rep(io, pos, prev);
            }
            self.litlen += 1;
            if self.litlen == 127 {
                self.litlen = 0;
                return Ok((127, 0, 0))
            }
        }

        let litlen = self.litlen;
        self.litlen = 0;
        Ok((litlen, 0, 0))
    }

    fn found_rep<IO: IOTrait + LookbackInput>(&mut self, io: &mut IO, pos: u64, prev: u64)
                                              -> BoxResult<(u8, u8, u64)> {
        let litlen_before = if self.litlen > 2 { self.litlen - 2 } else { 0 };
        let dist = pos - prev - 1;
        let mut matlen = 3;
        let mut prevpos = prev;
        self.litlen = 0;
        while let Some(b) = io.next_byte()? {
            self.update_hash(b, io.inpos() - 1);
            if b == io.lookback(prevpos) {
                matlen += 1;
                prevpos += 1;
            } else {
                self.litlen = 1;
                break;
            }
            if matlen == 127 { break };
        }
        return Ok((litlen_before, matlen, dist));
    }

    /// Updates the hash value, records the position at which we encountered
    /// it, and returns the previously most recent position for the same
    /// hash value.
    fn update_hash(&mut self, b: u8, pos: u64) -> u64 {
        self.hash = ((self.hash << 5) ^ b as u32) & self.hash_mask;
        let prev = self.pos[self.hash as usize];
        self.pos[self.hash as usize] = pos;
        prev
    }
}

fn write_lit<IO: IOTrait + LookbackInput>(io: &mut IO, litlen: u8, start: u64) -> BoxResult<()> {
    io.write_byte(litlen)?;
    let mut pos = start;
    for _ in 0..litlen {
        io.write_byte(io.lookback(pos))?;
        pos += 1;
    }
    Ok(())
}

pub fn encode<IO: IOTrait + LookbackInput>(io: &mut IO) -> BoxResult<()> {
    let mut state = EncoderState::new();
    loop {
        let pos = io.inpos() - state.litlen as u64;
        let (litlen, matlen, dist) = state.find_rep(io)?;
        if litlen == 0 && matlen == 0 { return Ok(()) }
        if litlen > 0 {
            write_lit(io, litlen, pos)?;
        }
        if matlen > 0 {
            io.write_byte(0x80 + matlen)?;
            io.write_byte((dist & 0xff) as u8)?;
            io.write_byte((dist >> 8) as u8)?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::SliceToVecIO;

    #[test]
    fn decode_empty() {
        let input = b"";
        let mut output = Vec::new();
        let mut io = SliceToVecIO::new(input, &mut output);
        decode(&mut io).unwrap();
        assert_eq!(output, b"");
    }

    #[test]
    fn decode_litlit() {
        let input = b"\x03xyz\x02zy";
        let mut output = Vec::new();
        let mut io = SliceToVecIO::new(input, &mut output);
        decode(&mut io).unwrap();
        assert_eq!(output, b"xyzzy");
    }

    #[test]
    fn decode_rep0() {
        let input = b"\x01a\x84\x00\x00";
        let mut output = Vec::new();
        let mut io = SliceToVecIO::new(input, &mut output);
        decode(&mut io).unwrap();
        assert_eq!(output, b"aaaaa");
    }

    #[test]
    fn decode_rep2() {
        let input = b"\x03abc\x83\x02\x00";
        let mut output = Vec::new();
        let mut io = SliceToVecIO::new(input, &mut output);
        decode(&mut io).unwrap();
        assert_eq!(output, b"abcabc");
    }

    #[test]
    fn decode_rep3() {
        let input = b"\x04abcd\x83\x03\x00";
        let mut output = Vec::new();
        let mut io = SliceToVecIO::new(input, &mut output);
        decode(&mut io).unwrap();
        assert_eq!(output, b"abcdabc");
    }

    #[test]
    fn encode_empty() {
        let input = b"";
        let mut output = Vec::new();
        let mut io = SliceToVecIO::new(input, &mut output);
        encode(&mut io).unwrap();
        assert_eq!(output, b"");
    }

    #[test]
    fn encode_lit() {
        let input = b"a";
        let mut output = Vec::new();
        let mut io = SliceToVecIO::new(input, &mut output);
        encode(&mut io).unwrap();
        assert_eq!(output, b"\x01a");
    }

    #[test]
    fn encode_litlit() {
        let input : Vec::<u8> = (0..=253).collect();
        let mut output = Vec::new();
        let mut io = SliceToVecIO::new(&input[..], &mut output);
        encode(&mut io).unwrap();
        let expected : Vec::<u8> = (0x7f..=0x7f).chain(0..=126)
            .chain(0x7f..=0x7f).chain(127..=253).collect();
        assert_eq!(output, expected);
    }
    
    #[test]
    fn encode_rep0() {
        let input = b"aaaaa";
        let mut output = Vec::new();
        let mut io = SliceToVecIO::new(input, &mut output);
        encode(&mut io).unwrap();
        assert_eq!(output, b"\x01a\x84\x00\x00");
    }

    #[test]
    fn encode_rep_lit() {
        let input = b"aaaaab";
        let mut output = Vec::new();
        let mut io = SliceToVecIO::new(input, &mut output);
        encode(&mut io).unwrap();
        assert_eq!(output, b"\x01a\x84\x00\x00\x01b");
    }
    
    #[test]
    fn encode_rep2() {
        let input = b"abcabc";
        let mut output = Vec::new();
        let mut io = SliceToVecIO::new(input, &mut output);
        encode(&mut io).unwrap();
        assert_eq!(output, b"\x03abc\x83\x02\x00");
    }

    #[test]
    fn encode_rep3() {
        let input = b"abcdabc";
        let mut output = Vec::new();
        let mut io = SliceToVecIO::new(input, &mut output);
        encode(&mut io).unwrap();
        assert_eq!(output, b"\x04abcd\x83\x03\x00");
    }
}
