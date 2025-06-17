use bitvec::vec::BitVec;

use crate::index::Index;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("bank is full")]
    Full,
    #[error("invalid index {0}")]
    InvalidIndex(u32),

    #[error("wrong generation for index {0}")]
    WrongGeneration(u32),
}

pub type Result<T> = std::result::Result<T, Error>;

type Generation = u8;

/// Out of 32 bits, 24 are for the index, 8 are for the generation
const MAX_INDEX: usize = (1 << 24) - 1;

pub struct Bank<T> {
    data: Vec<T>,
    present: BitVec,
    /// Current generation
    generation: Vec<Generation>,
    /// number of slots available in `data`
    available_slots: usize,
}

impl<T> Bank<T>
where
    T: Copy + Default,
{
    pub fn new() -> Self {
        Bank {
            data: vec![],
            present: BitVec::new(),
            generation: vec![],
            available_slots: 0,
        }
    }

    /// Number of items in the bank.
    pub fn len(&self) -> usize {
        self.data.len() - self.available_slots
    }

    pub fn get(&self, x: Index<T>) -> T {
        let idx = x.index();
        assert!(self.present[idx]);
        debug_assert_eq!(self.generation[idx], x.generation());
        self.data[idx]
    }

    pub fn try_get(&self, x: Index<T>) -> Option<T> {
        let idx = x.index();
        if idx >= self.data.len() {
            return None;
        }
        if !self.present[idx] {
            return None;
        }
        if self.generation[idx] != x.generation() {
            return None;
        }

        // SAFETY: entry is full because `present` is true
        Some(self.data[idx])
    }

    pub fn alloc(&mut self, x: T) -> Result<Index<T>> {
        let mut idx = usize::MAX;
        let mut generation = 0;
        if self.available_slots == 0 {
            // need to allocate a new slot
            debug_assert_eq!(self.data.len(), self.generation.len());
            debug_assert_eq!(self.data.len(), self.present.len());
            idx = self.data.len();

            if idx > MAX_INDEX as usize {
                return Err(Error::Full);
            }

            self.data.push(x);
            generation = 1;
            self.generation.push(generation);
            self.present.push(true);
        } else {
            // find a slot to reuse

            match self.present.iter_zeros().next() {
                Some(i) => {
                    self.present.set(i, true);
                    idx = i;
                    generation = self.generation[i];
                    self.available_slots -= 1;
                    self.data[i] = x;
                }
                None => unreachable!(),
            }
        }

        idx = (idx << 8) | (generation as usize);

        debug_assert!(idx <= (u32::MAX as usize));
        Ok(Index::from_u32(idx as u32))
    }

    #[inline]
    pub fn alloc_with(&mut self, f: impl FnOnce() -> T) -> Result<Index<T>> {
        let x = f();
        return self.alloc(x);
    }

    pub fn free(&mut self, x: Index<T>) -> Result<()> {
        let idx = x.index();
        if idx >= self.data.len() {
            return Err(Error::InvalidIndex(idx as u32));
        }

        if !self.present[idx] {
            return Err(Error::InvalidIndex(idx as u32));
        }

        let cur_gen = &mut self.generation[idx];
        if *cur_gen != x.generation() {
            return Err(Error::WrongGeneration(idx as u32));
        }

        *cur_gen = cur_gen.wrapping_add(1);
        self.present.set(idx, false);
        self.available_slots += 1;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_size() {
        assert_eq!(std::mem::size_of::<Generation>(), 1);
        assert_eq!(std::mem::size_of::<Index<i32>>(), 4);
        assert_eq!(std::mem::size_of::<Index<String>>(), 4);
    }

    #[test]
    fn it_allocates() {
        let mut bank: Bank<&'static str> = Bank::new();

        let str1 = bank.alloc("abc").unwrap();
        let str2 = bank.alloc("hello").unwrap();

        assert_eq!(2, bank.len());
        assert_ne!(str1, str2);

        assert_eq!("abc", bank.get(str1));
        assert_eq!("hello", bank.get(str2));

        bank.free(str1).unwrap();

        assert_eq!(None, bank.try_get(str1));
        assert_eq!("hello", bank.get(str2));

        let str3 = bank.alloc_with(|| "wowza").unwrap();
        assert_ne!(str2, str3);
        assert_ne!(str1, str3);
        assert_eq!("wowza", bank.get(str3));
    }

    #[test]
    fn lotsa_allocs() {
        let mut bank = Bank::new();

        const N: usize = 1_000_000;
        for _attempt in 0..2 {
            let mut v = vec![];
            for i in 0..N {
                let idx = bank.alloc(i).unwrap();
                v.push(idx);
            }
            assert_eq!(N, bank.len());

            for (i, &x) in v.iter().enumerate() {
                assert_eq!(i, bank.get(x));
            }

            for x in v.into_iter() {
                bank.free(x).unwrap();
            }
            assert_eq!(0, bank.len());
        }
    }
}
