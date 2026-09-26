//! Minimal CoAP Block1 (RFC 7959) option encode/decode for `no_std`.

/// Block option value (Block1 / Block2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockValue {
    pub num: u16,
    pub more: bool,
    /// SZX: block size = 2^(szx + 4). Valid range 0..=6.
    pub size_exponent: u8,
}

impl BlockValue {
    pub fn block_size(self) -> usize {
        1usize << (self.size_exponent as usize + 4)
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.is_empty() || bytes.len() > 3 {
            return None;
        }
        let mut n: u32 = 0;
        for &b in bytes {
            n = (n << 8) | u32::from(b);
        }
        let size_exponent = (n & 0x07) as u8;
        let more = (n & 0x08) != 0;
        let num = (n >> 4) as u16;
        if size_exponent > 6 {
            return None;
        }
        Some(Self {
            num,
            more,
            size_exponent,
        })
    }

    pub fn to_bytes(self) -> heapless::Vec<u8, 3> {
        let mut n = (u32::from(self.num) << 4)
            | (u32::from(self.more) << 3)
            | u32::from(self.size_exponent);
        let mut tmp = [0u8; 3];
        let mut len = 0usize;
        // Minimal-length encoding (drop leading zero bytes, keep at least 1).
        for i in (0..3).rev() {
            tmp[i] = (n & 0xff) as u8;
            n >>= 8;
            len += 1;
            if n == 0 {
                break;
            }
        }
        let start = 3 - len;
        let mut out = heapless::Vec::new();
        for &b in &tmp[start..] {
            let _ = out.push(b);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_small() {
        let b = BlockValue {
            num: 0,
            more: true,
            size_exponent: 6, // 1024
        };
        let bytes = b.to_bytes();
        assert_eq!(BlockValue::from_bytes(&bytes), Some(b));
    }
}
