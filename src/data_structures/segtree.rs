use std::fmt::Debug;
use std::iter::FromIterator;
use std::mem;
use derivative::Derivative;

pub trait SegTreeOp {
    type Item;
    type Lazy;

    fn apply(item: &mut Self::Item, lazy: Option<&mut Self::Lazy>, upper: &Self::Lazy);

    fn combine(left: &Self::Item, right: &Self::Item) -> Self::Item;

    fn no_lazy() -> Self::Lazy;
}

#[derive(Derivative)]
#[derivative(Debug(bound="O::Item: Debug, O::Lazy: Debug"))]
#[derivative(Clone(bound="O::Item: Clone, O::Lazy: Clone"))]
pub struct SegTree<O: SegTreeOp> {
    data: Vec<O::Item>,
    lazy: Vec<O::Lazy>,
}

impl<O: SegTreeOp> SegTree<O> {
    fn recalc_at(&mut self, index: usize) {
        self.data[index] = O::combine(&self.data[2 * index], &self.data[2 * index + 1]);
    }

    fn push(&mut self, index: usize) {
        let len = self.lazy.len();
        let height = 64 - (len as u64).leading_zeros();
        for shift in (1..=height).rev() {
            let idx = index >> shift;
            if idx == 0 {
                continue;
            }
            let (head, tail) = self.lazy.split_at_mut(idx + 1);
            O::apply(&mut self.data[2 * idx], tail.get_mut(idx - 1), &head[idx]);
            O::apply(&mut self.data[2 * idx + 1], tail.get_mut(idx), &head[idx]);
            head[idx] = O::no_lazy();
        }
    }

    pub fn change_single(&mut self, mut index: usize, op: impl FnOnce(&mut O::Item)) {
        index += self.lazy.len();
        self.push(index);
        op(&mut self.data[index]);
        while index > 1 {
            index /= 2;
            self.recalc_at(index);
        }
    }

    pub fn root(&self) -> &O::Item {
        &self.data[1]
    }

    pub fn apply_to_all(&mut self, lazy: &O::Lazy) {
        O::apply(&mut self.data[1], self.lazy.get_mut(1), lazy);
    }
}

impl<O: SegTreeOp> FromIterator<O::Item> for SegTree<O> where O::Item: Default {
    fn from_iter<T: IntoIterator<Item = O::Item>>(iter: T) -> Self {
        let iter = iter.into_iter();
        let mut data: Vec<_> = iter.collect();
        let len = data.len();
        assert!(len > 0, "SegTree cannot be initialized empty");
        data.reserve_exact(len);
        for idx in 0..len {
            let val = mem::take(&mut data[idx]);
            data.push(val);
        }

        let mut lazy = Vec::with_capacity(len);
        lazy.resize_with(len, O::no_lazy);
        let mut tree = SegTree { data, lazy };
        for index in (1..len).rev() {
            tree.recalc_at(index);
        }
        tree
    }
}
