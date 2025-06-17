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

        let idx_with_gen = (idx << 8) | (generation as usize);

        debug_assert!(idx_with_gen <= (u32::MAX as usize));
        Ok(Index::from_u32(idx_with_gen as u32))
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
