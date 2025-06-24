use bitvec::vec::BitVec;

pub use crate::error::{Error, Result};
use crate::index::Index;

type Generation = u8;

/// Out of 32 bits, 24 are for the index, 8 are for the generation
const MAX_INDEX: usize = (1 << 24) - 1;

union OrEmpty<T: Copy> {
    full: T,
    /// pointer for free list
    prev_empty: i32,
}

pub struct Bank<T: Copy> {
    data: Vec<OrEmpty<T>>,
    present: BitVec,
    /// Pointer to the beginning of empty list. negative means empty.
    last_empty: i32,
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
            last_empty: -1,
            generation: vec![],
            available_slots: 0,
        }
    }

    /// Number of items in the bank.
    #[inline]
    pub fn len(&self) -> usize {
        self.data.len() - self.available_slots
    }

    #[inline]
    pub fn get(&self, x: Index) -> T {
        let idx = x.index();
        assert!(self.present[idx]);
        debug_assert_eq!(self.generation[idx], x.generation());
        // SAFETY: entry is full because `present` is true
        unsafe { self.data[idx].full }
    }

    pub fn try_get(&self, x: Index) -> Option<T> {
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
        Some(unsafe { self.data[idx].full })
    }

    pub fn alloc(&mut self, x: T) -> Result<Index> {
        let idx;
        let generation;
        if self.available_slots == 0 {
            // need to allocate a new slot
            debug_assert_eq!(self.data.len(), self.generation.len());
            debug_assert_eq!(self.data.len(), self.present.len());
            idx = self.data.len();

            if idx > MAX_INDEX as usize {
                return Err(Error::Full);
            }

            self.data.push(OrEmpty { full: x });
            generation = 1;
            self.generation.push(generation);
            self.present.push(true);
        } else {
            // find a slot to reuse, using the linked list
            assert!(self.last_empty >= 0);
            idx = self.last_empty as usize;
            debug_assert!(!self.present[idx]);

            // SAFETY: `present` was false, so we have a list node
            self.last_empty = unsafe { self.data[idx].prev_empty };

            self.present.set(idx, true);
            generation = self.generation[idx];
            self.available_slots -= 1;
            self.data[idx] = OrEmpty { full: x };
            debug_assert!(idx != usize::MAX); // we must have found an index
        };

        let idx_with_gen = (idx << 8) | (generation as usize);

        debug_assert!(idx_with_gen <= (u32::MAX as usize));
        Ok(Index::from_u32(idx_with_gen as u32))
    }

    #[inline]
    pub fn alloc_with(&mut self, f: impl FnOnce() -> T) -> Result<Index> {
        let x = f();
        return self.alloc(x);
    }

    pub fn free(&mut self, x: Index) -> Result<()> {
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

        // remove the data. No need to drop it, it's Copy
        self.data[idx] = OrEmpty {
            prev_empty: self.last_empty,
        };

        self.last_empty = idx as i32;

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
