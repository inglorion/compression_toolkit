// Input/output abstractions used in compression_toolkit.
//
// Copyright 2021 Robbert Haarman
//
// SPDX-License-Identifier: MIT

use crate::result::BoxResult;

pub trait IOTrait {
    /// Copies count bytes from the input to the output.
    fn copy_bytes(&mut self, count: usize) -> BoxResult<()>;

    /// Returns the number of bytes of input that have been read so far.
    fn inpos(&self) -> u64;

    /// Attempts to read the next byte of input. Returns Ok(None) if
    /// the end of the input has been reached, Ok(Some(b)) when a byte
    /// b has been read and Err(e) if some error e has occurred.

    fn next_byte(&mut self) -> BoxResult<Option<u8>>;

    /// Writes a single byte to the output.
    fn write_byte(&mut self, b: u8) -> BoxResult<()>;
}

pub trait LookbackInput {
    /// Returns the byte at position pos, which must have been read
    /// previously.
    fn lookback(&self, pos: u64) -> u8;
}

pub trait ReadBits {
    fn read_bits(&mut self, nbits: u32) -> BoxResult<u32>;
}

pub trait RepeatOutput {
    /// Copies count previously output bytes to output, starting with the
    /// byte distance bytes before the most recently output byte.
    fn repeat_bytes(&mut self, count: usize, distance: usize) -> BoxResult<()>;
}

pub trait WriteBits {
    fn flush(&mut self) -> BoxResult<()>;
    fn write_bits(&mut self, bits: u32, nbits: u8) -> BoxResult<()>;
}

pub struct BitReader<'a> {
    input: &'a mut dyn std::io::Read,
    have_bits: u32,
    bits: u8,
}

impl<'a> BitReader<'a> {
    pub fn new(input: &'a mut dyn std::io::Read) -> BitReader {
        BitReader {
            input: input,
            bits: 0,
            have_bits: 0,
        }
    }
}

impl ReadBits for BitReader<'_> {
    fn read_bits(&mut self, nbits: u32) -> BoxResult<u32> {
        let mut bits = 0;
        let mut shift = 0;
        let mut need_bits = nbits;
        while need_bits > 0 {
            // If we are out of bits, read some.
            if self.have_bits == 0 {
                self.input.read_exact(std::slice::from_mut(&mut self.bits))?;
                self.have_bits = 8;
            }
            // Move bits from self.bits into bits.
            let n = std::cmp::min(need_bits, self.have_bits);
            bits |= ((self.bits as u32) & ((1 << n) - 1)) << shift;
            // Shifting an u8 by 8 panics in debug mode. Instead, shift as
            // an u32, which avoids the panic.
            self.bits = ((self.bits as u32) >> n) as u8;
            self.have_bits -= n;
            need_bits -= n;
            shift += n;
        }
        Ok(bits)
    }
}

pub struct BitWriter<'a> {
    /// Bytes will be written to this.
    output: &'a mut dyn std::io::Write,
    /// Accummulated bits.
    bits: u8,
    /// Number of accummulated bits.
    have_bits: u8,
}

impl<'a> BitWriter<'a> {
    pub fn new(output: &'a mut dyn std::io::Write) -> BitWriter<'a> {
        BitWriter {
            output: output,
            bits: 0,
            have_bits: 0,
        }
    }
}

impl WriteBits for BitWriter<'_> {
    fn flush(&mut self) -> BoxResult<()> {
        if self.have_bits > 0 {
            self.output.write(std::slice::from_ref(&self.bits))?;
            self.bits = 0;
            self.have_bits = 0;
        }
        Ok(())
    }
 
    fn write_bits(&mut self, bits: u32, nbits: u8) -> BoxResult<()> {
        let mut bits = bits;
        let mut nbits = nbits;
        while nbits > 0 {
            self.bits |= ((bits << self.have_bits) & 0xff) as u8;
            let need_bits = 8 - self.have_bits;
            if nbits < need_bits {
                self.have_bits += nbits;
                break;
            }
            self.output.write(std::slice::from_ref(&self.bits))?;
            self.bits = 0;
            self.have_bits = 0;
            bits >>= need_bits;
            nbits -= need_bits;
        }
        Ok(())
    }
}

pub struct SliceToVecIO<'a> {
    input: &'a [u8],
    output: &'a mut Vec::<u8>,
    inpos: usize,
}

impl<'a> SliceToVecIO<'a> {
    pub fn new(input: &'a [u8], output: &'a mut Vec::<u8>) -> SliceToVecIO<'a> {
        SliceToVecIO {
            input: input,
            output: output,
            inpos: 0,
        }
    }
}

impl IOTrait for SliceToVecIO<'_> {
    fn copy_bytes(&mut self, count: usize) -> BoxResult<()> {
        let newpos = self.inpos + count;
        self.output.extend_from_slice(&self.input[self.inpos..newpos]);
        self.inpos = newpos;
        Ok(())
    }

    fn inpos(&self) -> u64 { self.inpos as u64 }

    fn next_byte(&mut self) -> BoxResult<Option<u8>> {
        match self.input.get(self.inpos) {
            Some(b) => { self.inpos += 1; Ok(Some(*b)) },
            None => Ok(None),
        }
    }

    fn write_byte(&mut self, b: u8) -> BoxResult<()> {
        self.output.push(b);
        Ok(())
    }
}

impl LookbackInput for SliceToVecIO<'_> {
    fn lookback(&self, pos: u64) -> u8 { self.input[pos as usize] }
}

impl RepeatOutput for SliceToVecIO<'_> {
    fn repeat_bytes(&mut self, count: usize, dist: usize) -> BoxResult<()> {
        let mut outpos = self.output.len() - 1 - dist;
        for _ in 0..count {
            self.output.push(self.output[outpos]);
            outpos += 1;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitreader_empty() {
        let mut input = &b""[..];
        let mut reader = BitReader::new(&mut input);
        let r = reader.read_bits(1);
        assert!(r.is_err());
    }

    #[test]
    fn bitreader() {
        // From least significant to most significant:
        // 0101 0110 1010 0010 1000 0011 0110 1100
        let mut input = &b"jE\xc16"[..];
        let mut reader = BitReader::new(&mut input);
        // 0101
        assert_eq!(reader.read_bits(4).unwrap(), 0xa);
        // 011
        assert_eq!(reader.read_bits(3).unwrap(), 0x6);
        // 010
        assert_eq!(reader.read_bits(3).unwrap(), 0x2);
        // 1000 1010 0000 1101
        assert_eq!(reader.read_bits(16).unwrap(), 0xb051);
        // 1011 00
        assert_eq!(reader.read_bits(6).unwrap(), 0x0d);        
        assert!(reader.read_bits(1).is_err());        
    }

    #[test]
    fn bitwriter_5bits() {
        let mut output = Vec::new();
        let mut writer = BitWriter::new(&mut output);
        assert!(writer.write_bits(2, 2).is_ok());
        assert!(writer.write_bits(7, 3).is_ok());
        assert!(writer.flush().is_ok());
        assert_eq!(output, [0x1e]);
    }

    #[test]
    fn bitwriter_8bits() {
        let mut output = Vec::new();
        let mut writer = BitWriter::new(&mut output);
        assert!(writer.write_bits(2, 2).is_ok());
        assert!(writer.write_bits(7, 3).is_ok());
        assert!(writer.write_bits(5, 3).is_ok());
        assert!(writer.flush().is_ok());
        assert_eq!(output, [0xbe]);
    }

    #[test]
    fn bitwriter_9bits() {
        let mut output = Vec::new();
        let mut writer = BitWriter::new(&mut output);
        assert!(writer.write_bits(2, 2).is_ok());
        assert!(writer.write_bits(7, 3).is_ok());
        assert!(writer.write_bits(9, 4).is_ok());
        assert!(writer.flush().is_ok());
        assert_eq!(output, [0x3e, 0x01]);
    }
}
