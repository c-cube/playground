//! Bank for arrays and slices

pub use crate::error::{Error, Result};
use crate::{bank::Bank, index::Index};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ArrayIndex {
    idx: Index,
    len: u32,
}

pub struct ArrayBank<T> {
    bank1: Bank<[T; 1]>,
    bank2: Bank<[T; 2]>,
    bank3: Bank<[T; 3]>,
    bank_n: Bank<Box<[T]>>,
}

#[inline]
fn const_slice_to<T, const N: usize>(x: &[T]) -> [T; N]
where
    T: Clone,
{
    let arr: &[T; N] = x.try_into().unwrap();
    arr.clone()
}

impl<T> ArrayBank<T>
where
    T: Clone,
{
    pub fn new() -> Self {
        Self {
            bank1: Bank::new(),
            bank2: Bank::new(),
            bank3: Bank::new(),
            bank_n: Bank::new(),
        }
    }

    /// Number of items in the bank.
    #[inline]
    pub fn len(&self) -> usize {
        self.bank1.len() + self.bank2.len() + self.bank3.len() + self.bank_n.len()
    }

    pub fn get(&self, x: ArrayIndex) -> &[T] {
        match x.len {
            0 => &[],
            1 => self.bank1.get(x.idx).as_slice(),
            2 => self.bank2.get(x.idx).as_slice(),
            3 => self.bank3.get(x.idx).as_slice(),
            _ => &*self.bank_n.get(x.idx),
        }
    }

    pub fn try_get(&self, x: ArrayIndex) -> Option<&[T]> {
        Some(match x.len {
            0 => &[],
            1 => self.bank1.try_get(x.idx)?.as_slice(),
            2 => self.bank2.try_get(x.idx)?.as_slice(),
            3 => self.bank3.try_get(x.idx)?.as_slice(),
            _ => &*self.bank_n.try_get(x.idx)?,
        })
    }

    pub fn alloc(&mut self, x: &[T]) -> Result<ArrayIndex> {
        if x.len() > u32::MAX as usize {
            return Err(Error::SliceTooBig);
        }

        let len = x.len();
        let idx = match len {
            0 => Index::from_u32(0),
            1 => self.bank1.alloc(const_slice_to(x))?,
            2 => self.bank2.alloc(const_slice_to(x))?,
            3 => self.bank3.alloc(const_slice_to(x))?,
            _ => {
                let v: Vec<_> = x.iter().cloned().collect();
                self.bank_n.alloc(v.into_boxed_slice())?
            }
        };
        Ok(ArrayIndex {
            idx,
            len: len as u32,
        })
    }

    pub fn alloc_iter(&mut self, i: impl IntoIterator<Item = T>) -> Result<ArrayIndex> {
        let v: Vec<_> = i.into_iter().collect();
        self.alloc(&v)
    }

    pub fn free(&mut self, x: ArrayIndex) -> Result<()> {
        match x.len {
            0 => (),
            1 => self.bank1.free(x.idx)?,
            2 => self.bank2.free(x.idx)?,
            3 => self.bank3.free(x.idx)?,
            _ => self.bank_n.free(x.idx)?,
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn alloc_some() {
        let mut bank = ArrayBank::new();

        let a1 = bank.alloc(&[1, 2, 3]).unwrap();
        let a2 = bank.alloc(&[1]).unwrap();
        let a3 = bank.alloc(&[42]).unwrap();

        assert_eq!(3, bank.len());

        assert_eq!(bank.get(a1), &[1, 2, 3]);
        assert_eq!(bank.get(a3), &[42]);
        assert_eq!(bank.get(a2), &[1]);
    }
}
