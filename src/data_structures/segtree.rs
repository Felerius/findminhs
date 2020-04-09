use std::fmt::Debug;

pub trait SegTreeOp {
    type Item;
    type Lazy;

    fn apply(&mut self, item: &mut Self::Item, lazy: Option<&mut Self::Lazy>, upper: &Self::Lazy);

    fn combine(&mut self, left: &Self::Item, right: &Self::Item) -> Self::Item;

    fn no_lazy() -> Self::Lazy;
}

#[derive(Debug, Clone)]
pub struct SegTree<O: SegTreeOp> {
    op: O,
    data: Vec<O::Item>,
    lazy: Vec<O::Lazy>,
}

impl<O: SegTreeOp> SegTree<O> {
    fn recalc_at(&mut self, index: usize) {
        self.data[index] = self
            .op
            .combine(&self.data[2 * index], &self.data[2 * index + 1]);
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
            self.op
                .apply(&mut self.data[2 * idx], tail.get_mut(idx - 1), &head[idx]);
            self.op
                .apply(&mut self.data[2 * idx + 1], tail.get_mut(idx), &head[idx]);
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
        self.op.apply(&mut self.data[1], self.lazy.get_mut(1), lazy);
    }
}

impl<O: SegTreeOp> SegTree<O>
where
    O::Lazy: Clone,
{
    pub fn from_iter<I>(op: O, iter: I) -> Self
    where
        I: IntoIterator<Item = O::Item>,
        I::IntoIter: ExactSizeIterator,
        O::Item: Default,
    {
        let iter = iter.into_iter();
        let len = iter.len();
        assert!(len > 0, "SegTree cannot be initialized empty");
        let mut data = Vec::with_capacity(2 * len);
        data.resize_with(len, O::Item::default);
        data.extend(iter);

        let lazy = vec![O::no_lazy(); len];
        let mut tree = SegTree { op, data, lazy };
        for index in (1..len).rev() {
            tree.recalc_at(index);
        }
        tree
    }
}
