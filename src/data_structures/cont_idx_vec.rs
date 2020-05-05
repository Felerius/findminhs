use crate::create_idx_struct;
use crate::small_indices::SmallIdx;
use std::iter::FromIterator;
use std::ops::Deref;

create_idx_struct!(DataIdx);

/// Stores a indices contiguously and allows O(1) deletion/restoration by index.
///
/// Allows for:
///  * random access to the non-deleted indices (via `Deref<Target=[T]>`).
///    However, the order of the indices themselves is not fixed.
///  * O(1) deletion and restoration given an index
///
/// This is achieved by a index to position indirection table. Deletion/
/// restoration is implemented by swapping, partitioning the deleted elements
/// after all non-deleted.
pub struct ContiguousIdxVec<T> {
    data: Vec<T>,
    indices: Vec<DataIdx>,
    len: usize,
}

impl<T: Into<usize> + Copy> ContiguousIdxVec<T> {
    pub fn is_deleted(&self, id: usize) -> bool {
        self.indices[id].idx() >= self.len
    }

    pub fn delete(&mut self, id: usize) {
        debug_assert!(
            !self.is_deleted(id),
            "Item with id {} was already deleted",
            id
        );
        let idx = self.indices[id].idx();
        let last_id = self.data[self.len - 1].into();
        self.data.swap(idx, self.len - 1);
        self.indices.swap(id, last_id);
        self.len -= 1;
    }

    pub fn restore(&mut self, id: usize) {
        debug_assert!(self.is_deleted(id), "Item with id {} is not deleted", id);
        let idx = self.indices[id].idx();
        let after_last_id = self.data[self.len].into();
        self.data.swap(idx, self.len);
        self.indices.swap(id, after_last_id);
        self.len += 1;
    }
}

impl<T> FromIterator<T> for ContiguousIdxVec<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let data: Vec<_> = iter.into_iter().collect();
        let indices = (0..data.len()).map(DataIdx::from).collect();
        let len = data.len();
        Self { data, indices, len }
    }
}

impl<T> Deref for ContiguousIdxVec<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.data[..self.len]
    }
}
