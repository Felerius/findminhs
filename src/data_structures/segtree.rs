use crate::create_idx_struct;
use crate::small_indices::SmallIdx;
use derivative::Derivative;
use std::fmt::Debug;
use std::iter::FromIterator;

pub trait SegTreeOp {
    type Item;

    fn combine(left: &Self::Item, right: &Self::Item) -> Self::Item;
}

#[derive(Derivative)]
#[derivative(Debug(bound = "O::Item: Debug"))]
#[derivative(Clone(bound = "O::Item: Clone"))]
pub struct SegTree<O: SegTreeOp> {
    data: Vec<O::Item>,
}

create_idx_struct!(HeapIdx);

impl HeapIdx {
    fn left(self) -> Self {
        Self::from(2 * self.0 + 1)
    }

    fn right(self) -> Self {
        Self::from(2 * self.0 + 2)
    }

    fn parent(self) -> Self {
        Self::from((self.0 - 1) / 2)
    }

    fn has_parent(self) -> bool {
        self.0 > 0
    }
}

impl<O: SegTreeOp> SegTree<O> {
    fn first_leaf(&self) -> usize {
        self.data.len() / 2
    }

    fn recalc_at(&mut self, index: HeapIdx) {
        self.data[index.idx()] = O::combine(
            &self.data[index.left().idx()],
            &self.data[index.right().idx()],
        );
    }

    pub fn change(&mut self, index: usize, op: impl FnOnce(&mut O::Item)) {
        let mut index = HeapIdx::from(index + self.first_leaf());
        op(&mut self.data[index.idx()]);
        while index.has_parent() {
            index = index.parent();
            self.recalc_at(index);
        }
    }

    pub fn change_all(&mut self, mut op: impl FnMut(&mut O::Item)) {
        let first_leaf = self.first_leaf();
        for item in self.data[first_leaf..].iter_mut().rev() {
            op(item);
        }
        for index in (0..first_leaf).map(HeapIdx::from).rev() {
            self.recalc_at(index);
        }
    }

    pub fn set(&mut self, index: usize, value: O::Item) {
        self.change(index, |val| *val = value);
    }

    pub fn root(&self) -> &O::Item {
        &self.data[0]
    }
}

impl<O: SegTreeOp> FromIterator<O::Item> for SegTree<O>
where
    O::Item: Default,
{
    fn from_iter<T: IntoIterator<Item = O::Item>>(iter: T) -> Self {
        let mut data: Vec<_> = iter.into_iter().collect();
        let len = data.len();
        let tree_size = 2 * len - 1;
        data.resize_with(tree_size, Default::default);
        data.rotate_right(tree_size - len);

        for index in (0..(len - 1)).map(HeapIdx::from).rev() {
            data[index.idx()] = O::combine(&data[index.left().idx()], &data[index.right().idx()]);
        }

        Self { data }
    }
}
