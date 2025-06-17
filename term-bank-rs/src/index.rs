#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct Index {
    idx: u32,
}

impl Index {
    #[inline]
    pub fn index(self) -> usize {
        (self.idx >> 8) as usize
    }

    #[inline]
    pub fn generation(self) -> u8 {
        (self.idx & 0b1111_1111) as u8
    }

    #[inline]
    pub(crate) fn from_u32(i: u32) -> Self {
        Self { idx: i }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_size() {
        assert_eq!(std::mem::size_of::<Index>(), 4);
    }
}
