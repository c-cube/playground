use std::mem::ManuallyDrop;

pub use crate::error::{Error, Result};
use crate::index::Index;
use bitvec::vec::BitVec;

type Generation = u8;

const MAX_INDEX: usize = (1 << 24) - 1;

union OrEmpty<T> {
    full: ManuallyDrop<T>,
    /// pointer for free list
    prev_empty: i32,
}

pub struct Bank<T> {
    data: Vec<OrEmpty<T>>,
    present: BitVec,
    /// Pointer to the beginning of empty list. negative means empty.
    last_empty: i32,
    /// Current generation
    generation: Vec<Generation>,
    /// number of slots available in `data`
    available_slots: usize,
}

impl<T> Bank<T> {
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
    pub fn get(&self, x: Index) -> &T {
        let idx = x.index();
        assert!(self.present[idx]);
        debug_assert_eq!(self.generation[idx], x.generation());
        // SAFETY: entry is full because `present` is true
        unsafe { &self.data[idx].full }
    }

    pub fn try_get(&self, x: Index) -> Option<&T> {
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
        Some(unsafe { &self.data[idx].full })
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

            self.data.push(OrEmpty {
                full: ManuallyDrop::new(x),
            });
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
            self.data[idx] = OrEmpty {
                full: ManuallyDrop::new(x),
            };
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

        {
            // remove the data, by swapping it with a list element
            let mut local_or_empty = OrEmpty {
                prev_empty: self.last_empty,
            };
            std::mem::swap(&mut self.data[idx], &mut local_or_empty);

            // SAFETY: `present` is true, so this must be full
            unsafe { ManuallyDrop::drop(&mut local_or_empty.full) };
        }
        self.last_empty = idx as i32;

        *cur_gen = cur_gen.wrapping_add(1);
        self.present.set(idx, false);
        self.available_slots += 1;

        Ok(())
    }
}

impl<T> Drop for Bank<T> {
    fn drop(&mut self) {
        // TODO: drop all items with `present` = true
        for (i, data) in self.data.iter_mut().enumerate() {
            if self.present[i] {
                // put garbage there
                let mut local_or_empty = OrEmpty { prev_empty: 0 };
                std::mem::swap(data, &mut local_or_empty);
                self.present.set(i, false);

                // SAFETY: present is true, so this must be full
                unsafe { ManuallyDrop::drop(&mut local_or_empty.full) };
            }
        }
    }
}

impl<T> Bank<T>
where
    T: Copy,
{
    /// Get a copy at given index.
    #[inline]
    pub fn get_copy(&self, x: Index) -> T {
        *self.get(x)
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
        let mut bank: Bank<String> = Bank::new();

        let str1 = bank.alloc("abc".to_string()).unwrap();
        let str2 = bank.alloc("hello".to_string()).unwrap();

        assert_eq!(2, bank.len());
        assert_ne!(str1, str2);

        assert_eq!("abc", bank.get(str1));
        assert_eq!("hello", bank.get(str2));

        bank.free(str1).unwrap();

        assert_eq!(None, bank.try_get(str1));
        assert_eq!("hello", bank.get(str2));

        let str3 = bank.alloc_with(|| "wowza".to_string()).unwrap();
        assert_ne!(str2, str3);
        assert_ne!(str1, str3);
        assert_eq!("wowza", bank.get(str3));
    }

    #[test]
    fn lotsa_allocs() {
        let mut bank = Bank::new();

        const N: usize = 1_000_000;
        for _attempt in 0..10 {
            let mut v = vec![];
            for i in 0..N {
                let idx = bank.alloc(i).unwrap();
                v.push(idx);
            }
            assert_eq!(N, bank.len());

            for (i, &x) in v.iter().enumerate() {
                assert_eq!(i, bank.get_copy(x));
            }

            for x in v.into_iter() {
                bank.free(x).unwrap();
            }
            assert_eq!(0, bank.len());
        }
    }
}
