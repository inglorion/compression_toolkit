const STATE_BITS : u32 = 24;
const PROB_BITS : u32 = 8;
const MAX_RANGE : u32 = (1 << STATE_BITS) - 1;
const NORM_SHIFT : u32 = STATE_BITS - 8;
const NORM_MASK : u32 = (1 << NORM_SHIFT) - 1;

fn compute_threshold(range: u32, p0: u8) -> u32 {
    (range * p0 as u32) >> PROB_BITS
}

fn normalize_needed(low: u32, range: u32) -> bool {
    (low & NORM_MASK) + range <= NORM_MASK
}

pub struct Decoder {
    low: u32,
    range: u32,
    code: u32,
}

impl Decoder {
    pub fn new() -> Decoder {
        Decoder {
            low: 0,
            range: 0,
            code: 0,
        }
    }

    pub fn decode_bit(&mut self, p0: u8) -> bool {
        let threshold = compute_threshold(self.range, p0);
        let bit = self.code > threshold;
        if bit {
            self.code -= threshold + 1;
            self.range -= threshold + 1;
            self.low += threshold + 1;
        } else {
            self.range = threshold;
        }
        bit
    }

    pub fn needs_normalize(&self) -> bool {
        normalize_needed(self.low, self.range)
    }

    pub fn normalize(&mut self, b: u8) {
        self.range = (self.range << 8) | 0xff;
        self.code = (self.code << 8) | b as u32;
        self.low <<= 8;
    }
}

pub struct Encoder {
    low: u32,
    range: u32,
}

impl Encoder {
    pub fn new() -> Encoder {
        Encoder {
            low: 0,
            range: MAX_RANGE,
        }
    }

    pub fn encode_bit(&mut self, p0: u8, bit: bool) {
        let threshold = compute_threshold(self.range, p0);
        if bit {
            self.low += threshold + 1;
            self.range -= threshold + 1;
        } else {
            self.range = threshold;
        }
    }

    pub fn needs_normalize(&self) -> bool {
        normalize_needed(self.low, self.range)
    }

    pub fn normalize(&mut self) -> u8 {
        let out = (self.low >> NORM_SHIFT) as u8;
        self.range = (self.range << 8) | 0xff;
        self.low <<= 8;
        out
    }

    pub fn flush(&mut self) -> u8 {
        ((self.low + self.range) >> NORM_SHIFT) as u8
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_hi() {
        let mut decoder = Decoder::new();
        assert_eq!(decoder.needs_normalize(), true);
        decoder.normalize(0x73);
        assert_eq!(decoder.needs_normalize(), true);
        decoder.normalize(0xe4);
        assert_eq!(decoder.needs_normalize(), true);
        decoder.normalize(0x00);
        assert_eq!(decoder.needs_normalize(), false);
        assert_eq!(decoder.decode_bit(160), false);
        assert_eq!(decoder.decode_bit(160), true);
        assert_eq!(decoder.decode_bit(160), false);
        assert_eq!(decoder.decode_bit(160), false);
        assert_eq!(decoder.decode_bit(160), true);
        assert_eq!(decoder.decode_bit(160), false);
        assert_eq!(decoder.decode_bit(160), false);
        assert_eq!(decoder.decode_bit(160), false);
        assert_eq!(decoder.decode_bit(160), false);
        assert_eq!(decoder.needs_normalize(), false);
        assert_eq!(decoder.decode_bit(160), true);
        assert_eq!(decoder.needs_normalize(), true);
        decoder.normalize(0x00);
        assert_eq!(decoder.decode_bit(160), true);
        assert_eq!(decoder.decode_bit(160), false);
        assert_eq!(decoder.decode_bit(160), true);
        assert_eq!(decoder.decode_bit(160), false);
        assert_eq!(decoder.decode_bit(160), false);
        assert_eq!(decoder.decode_bit(160), true);
    }

    #[test]
    fn encode_hi() {
        let mut encoder = Encoder::new();
        encoder.encode_bit(160, false);
        encoder.encode_bit(160, true);
        encoder.encode_bit(160, false);
        encoder.encode_bit(160, false);
        encoder.encode_bit(160, true);
        encoder.encode_bit(160, false);
        encoder.encode_bit(160, false);
        encoder.encode_bit(160, false);
        encoder.encode_bit(160, false);
        assert_eq!(encoder.needs_normalize(), false);
        encoder.encode_bit(160, true);
        assert_eq!(encoder.needs_normalize(), true);
        assert_eq!(encoder.normalize(), 0x73);
        encoder.encode_bit(160, true);
        encoder.encode_bit(160, false);
        encoder.encode_bit(160, true);
        encoder.encode_bit(160, false);
        encoder.encode_bit(160, false);
        encoder.encode_bit(160, true);
        assert_eq!(encoder.needs_normalize(), false);
        assert_eq!(encoder.flush(), 0xe4);
    }

    #[test]
    fn english() {
        struct Predictor {
            bit: u8,
        }

        impl Predictor {
            pub fn new() -> Predictor {
                Predictor { bit: 0 }
            }

            pub fn p0(&mut self) -> u8 {
                let p = match 7 - self.bit {
                    7 => 255,
                    6 => 51,
                    5 => 20,
                    _ => 128,
                };
                self.bit = (self.bit + 1) & 7;
                p
            }
        }

        let input = b"Hello, world!\n";
        let mut output = Vec::new();
        
        let mut predictor = Predictor::new();
        let mut encoder = Encoder::new();
        
        for b in input {
            for i in 0..8 {
                let bit = b & (1 << (7 - i)) != 0;
                encoder.encode_bit(predictor.p0(), bit);
                if encoder.needs_normalize() {
                    output.push(encoder.normalize());
                }
            }
        }
        output.push(encoder.flush());
        assert_eq!(output, b"6\xfb\x8dkd>\x16\xaf#\xd8\xfa");

        let mut predictor = Predictor::new();
        let mut decoder = Decoder::new();
        let mut input = output.iter();
        let mut output = Vec::new();
        for _ in 0..14 {
            let mut b = 0;
            for i in 0..8 {
                while decoder.needs_normalize() {
                    decoder.normalize(*input.next().unwrap_or(&0));
                }
                b |= (decoder.decode_bit(predictor.p0()) as u8) << (7 - i);
            }
            output.push(b);
        }

        assert_eq!(output, b"Hello, world!\n");
    }
}
