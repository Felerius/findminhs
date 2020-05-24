use crate::create_idx_struct;
use crate::small_indices::SmallIdx;
use std::convert::TryFrom;
use std::fmt::{self, Debug, Formatter};
use std::iter::{self, FromIterator, FusedIterator};
use std::ops::{Index, IndexMut};

/// Fixed-size vector that can delete and restore elements in O(1).
///
/// Internally, a doubly-linked list between non-deleted elements is kept. To
/// conserve space, restoration of elements is only allowed in the reverse
/// order of the corresponding deletions. Deleted elements can still be
/// accessed, but will be skipped while iterating, and are not accounted for
/// in `len()`.
///
/// To conserve additional space, all indices are kept as `u32`'s internally.
#[derive(Clone)]
pub struct SkipVec<T> {
    entries: Box<[Entry<T>]>,
    first: EntryIdx,
    last: EntryIdx,
    len: u32,
    #[cfg(feature = "debug-skipvec")]
    deletions: Vec<EntryIdx>,
}

#[derive(Debug, Clone, Default)]
struct Entry<T> {
    prev: EntryIdx,
    next: EntryIdx,
    value: T,
    #[cfg(feature = "debug-skipvec")]
    deleted: bool,
}

/// Iterator over an `SkipVec<T>`.
#[derive(Debug, Clone)]
pub struct Iter<'a, T> {
    list: &'a SkipVec<T>,
    front: EntryIdx,
    back: EntryIdx,
    rem_len: u32,
}

/// Mutable iterator over an `SkipVec<T>`.
#[derive(Debug)]
pub struct IterMut<'a, T> {
    list: &'a mut SkipVec<T>,
    front: EntryIdx,
    back: EntryIdx,
    rem_len: u32,
}

create_idx_struct!(EntryIdx);

impl<T> Entry<T> {
    fn new(value: T) -> Self {
        Self {
            prev: EntryIdx::INVALID,
            next: EntryIdx::INVALID,
            value,
            #[cfg(feature = "debug-skipvec")]
            deleted: false,
        }
    }
}

impl<T> SkipVec<T> {
    #[cfg(feature = "debug-skipvec")]
    fn check_invariants(&self) {
        let mut idx = self.first;
        while idx.valid() {
            let next = self.entries[idx.idx()].next;
            if next.valid() {
                let prev_of_next = self.entries[next.idx()].prev;
                debug_assert_eq!(
                    idx, prev_of_next,
                    "Invariant violated: next of {} is {}, but prev of {} is {}",
                    idx, next, next, prev_of_next
                );
            }
            idx = next;
        }
        if self.first.valid() {
            debug_assert_eq!(
                self.entries[self.first.idx()].prev,
                EntryIdx::INVALID,
                "Invariant violated: prev of first is not invalid",
            );
        }
        if self.last.valid() {
            debug_assert_eq!(
                self.entries[self.last.idx()].next,
                EntryIdx::INVALID,
                "Invariant violated: next of last is not invalid",
            );
        }
    }

    fn from_entry_vec(mut vec: Vec<Entry<T>>) -> Self {
        for (idx, entry) in vec.iter_mut().enumerate() {
            entry.prev = idx.checked_sub(1).map_or(EntryIdx::INVALID, EntryIdx::from);
            entry.next = EntryIdx::from(idx + 1);
        }
        if let Some(entry) = vec.last_mut() {
            entry.next = EntryIdx::INVALID;
        }
        debug_assert!(
            u32::try_from(vec.len()).is_ok(),
            "SkipVec size must fit a u32"
        );
        let len = vec.len() as u32;
        let (first, last) = if vec.is_empty() {
            (EntryIdx::INVALID, EntryIdx::INVALID)
        } else {
            (EntryIdx(0), EntryIdx(len - 1))
        };
        #[cfg_attr(not(feature = "debug-skipvec"), allow(clippy::let_and_return))]
        let instance = Self {
            entries: vec.into_boxed_slice(),
            first,
            last,
            len,
            #[cfg(feature = "debug-skipvec")]
            deletions: vec![],
        };
        #[cfg(feature = "debug-skipvec")]
        instance.check_invariants();
        instance
    }

    pub fn try_sorted_from<E>(iter: impl IntoIterator<Item = Result<T, E>>) -> Result<Self, E>
    where
        T: Ord,
    {
        let mut vec = iter
            .into_iter()
            .map(|result| result.map(Entry::new))
            .collect::<Result<Vec<_>, _>>()?;
        vec.sort_unstable_by(|entry1, entry2| entry1.value.cmp(&entry2.value));
        Ok(Self::from_entry_vec(vec))
    }

    pub fn with_len(len: usize) -> Self
    where
        T: Default,
    {
        iter::repeat_with(T::default).take(len).collect()
    }

    pub fn iter(&self) -> Iter<'_, T> {
        self.into_iter()
    }

    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        self.into_iter()
    }

    /// Length of the linked list.
    pub fn len(&self) -> usize {
        self.len as usize
    }

    pub fn first(&self) -> Option<usize> {
        self.first.idx_if_valid()
    }

    pub fn next(&self, idx: usize) -> Option<usize> {
        self.entries[idx].next.idx_if_valid()
    }

    pub fn prev(&self, idx: usize) -> Option<usize> {
        self.entries[idx].prev.idx_if_valid()
    }

    /// Delete the item with the given index.
    ///
    /// This can corrupt the list if the item was already deleted.
    pub fn delete(&mut self, index: usize) {
        #[cfg(feature = "debug-skipvec")]
        {
            debug_assert!(
                !self.entries[index].deleted,
                "Entry {} already deleted",
                index
            );
            self.entries[index].deleted = true;
        }
        let Entry { prev, next, .. } = self.entries[index];
        self.len -= 1;
        if prev.valid() {
            debug_assert_eq!(self.entries[prev.idx()].next, EntryIdx::from(index));
            self.entries[prev.idx()].next = next;
        } else {
            debug_assert_eq!(self.first, EntryIdx::from(index));
            self.first = next;
        }
        if next.valid() {
            debug_assert_eq!(self.entries[next.idx()].prev, EntryIdx::from(index));
            self.entries[next.idx()].prev = prev;
        } else {
            debug_assert_eq!(self.last, EntryIdx::from(index));
            self.last = prev;
        }
        #[cfg(feature = "debug-skipvec")]
        {
            self.deletions.push(EntryIdx::from(index));
            self.check_invariants();
        }
    }

    /// Restore a deleted item.
    ///
    /// This operation only produces correct results if the restorations are
    /// done in the reverse order of the corresponding deletions. Otherwise,
    /// the results will be unpredictable (but still memory-safe).
    pub fn restore(&mut self, index: usize) {
        #[cfg(feature = "debug-skipvec")]
        {
            let popped = self.deletions.pop();
            debug_assert_eq!(
                popped,
                Some(EntryIdx::from(index)),
                "Restorations out-of-order: expected {:?} next, but got {}",
                popped,
                index
            );
            debug_assert!(
                self.entries[index].deleted,
                "Entry {} already restored",
                index
            );
            self.entries[index].deleted = false;
        }
        let Entry { prev, next, .. } = self.entries[index];
        self.len += 1;
        if prev.valid() {
            debug_assert_eq!(self.entries[prev.idx()].next, next);
            self.entries[prev.idx()].next = EntryIdx::from(index);
        } else {
            debug_assert_eq!(self.first, next);
            self.first = EntryIdx::from(index);
        }
        if next.valid() {
            debug_assert_eq!(self.entries[next.idx()].prev, prev);
            self.entries[next.idx()].prev = EntryIdx::from(index);
        } else {
            debug_assert_eq!(self.last, prev);
            self.last = EntryIdx::from(index);
        }
        #[cfg(feature = "debug-skipvec")]
        {
            self.check_invariants();
        }
    }
}

impl<T: Debug> Debug for SkipVec<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<T> Default for SkipVec<T> {
    fn default() -> Self {
        Self {
            entries: vec![].into_boxed_slice(),
            first: EntryIdx::INVALID,
            last: EntryIdx::INVALID,
            len: 0,
            #[cfg(feature = "debug-skipvec")]
            deletions: vec![],
        }
    }
}

impl<T> FromIterator<T> for SkipVec<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self::from_entry_vec(iter.into_iter().map(Entry::new).collect())
    }
}

impl<T> Index<usize> for SkipVec<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        &self.entries[index].value
    }
}

impl<T> IndexMut<usize> for SkipVec<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.entries[index].value
    }
}

impl<'a, T> IntoIterator for &'a SkipVec<T> {
    type Item = (usize, &'a T);
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        Iter {
            list: self,
            front: self.first,
            back: self.last,
            rem_len: self.len,
        }
    }
}

impl<'a, T> IntoIterator for &'a mut SkipVec<T> {
    type Item = (usize, &'a mut T);
    type IntoIter = IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        let front = self.first;
        let back = self.last;
        let rem_len = self.len;
        IterMut {
            list: self,
            front,
            back,
            rem_len,
        }
    }
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = (usize, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        if !self.front.valid() {
            return None;
        }
        let index = self.front.idx();
        let entry = &self.list.entries[index];
        self.front = if self.front == self.back {
            EntryIdx::INVALID
        } else {
            entry.next
        };
        self.rem_len -= 1;
        Some((index, &entry.value))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.rem_len as usize, Some(self.rem_len as usize))
    }
}

impl<'a, T> DoubleEndedIterator for Iter<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if !self.front.valid() {
            return None;
        }
        let index = self.back.idx();
        let entry = &self.list.entries[index];
        if self.front == self.back {
            self.front = EntryIdx::INVALID;
        } else {
            self.back = entry.prev;
        }
        self.rem_len -= 1;
        Some((index, &entry.value))
    }
}

impl<'a, T> FusedIterator for Iter<'a, T> {}

impl<'a, T> ExactSizeIterator for Iter<'a, T> {}

impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = (usize, &'a mut T);

    fn next(&mut self) -> Option<Self::Item> {
        if !self.front.valid() {
            return None;
        }
        let index = self.front.idx();
        let entry = &mut self.list.entries[index];
        self.front = if self.front == self.back {
            EntryIdx::INVALID
        } else {
            entry.next
        };
        self.rem_len -= 1;
        // Unsafe reborrow to get 'a lifetime
        Some((index, unsafe { &mut *(&mut entry.value as *mut T) }))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.rem_len as usize, Some(self.rem_len as usize))
    }
}

impl<'a, T> DoubleEndedIterator for IterMut<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if !self.front.valid() {
            return None;
        }
        let index = self.back.idx();
        let entry = &mut self.list.entries[index];
        if self.front == self.back {
            self.front = EntryIdx::INVALID;
        } else {
            self.back = entry.prev;
        };
        self.rem_len -= 1;
        // Unsafe reborrow to get 'a lifetime
        Some((index, unsafe { &mut *(&mut entry.value as *mut T) }))
    }
}

impl<'a, T> FusedIterator for IterMut<'a, T> {}

impl<'a, T> ExactSizeIterator for IterMut<'a, T> {}
