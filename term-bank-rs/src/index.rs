pub struct Index<T> {
    idx: u32,
    phantom: PhantomData<*const T>,
}

use std::marker::PhantomData;

impl<T> Copy for Index<T> {}
impl<T> Clone for Index<T> {
    fn clone(&self) -> Self {
        Self {
            idx: self.idx.clone(),
            phantom: self.phantom.clone(),
        }
    }
}

impl<T> Eq for Index<T> {}
impl<T> PartialEq for Index<T> {
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx
    }
}

impl<T> std::hash::Hash for Index<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.idx.hash(state);
    }
}
impl<T> std::fmt::Debug for Index<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.idx.fmt(f)
    }
}

impl<T> Index<T> {
    #[inline]
    pub fn index(self) -> usize {
        (self.idx >> 8) as usize
    }

    #[inline]
    pub fn generation(self) -> u8 {
        (self.idx & 0b1111_1111) as u8
    }

    pub(crate) fn from_u32(i: u32) -> Self {
        Self {
            idx: i,
            phantom: PhantomData,
        }
    }
}
