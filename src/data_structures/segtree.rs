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

fn left_child(index: usize) -> usize {
    2 * index + 1
}

fn right_child(index: usize) -> usize {
    2 * index + 2
}

fn parent(index: usize) -> usize {
    (index - 1) / 2
}

impl<O: SegTreeOp> SegTree<O> {
    fn first_leaf(&self) -> usize {
        self.data.len() / 2
    }

    fn recalc_at(&mut self, index: usize) {
        self.data[index] = O::combine(
            &self.data[left_child(index)],
            &self.data[right_child(index)],
        );
    }

    pub fn change(&mut self, mut index: usize, op: impl FnOnce(&mut O::Item)) {
        index += self.first_leaf();
        op(&mut self.data[index]);
        while index > 0 {
            index = parent(index);
            self.recalc_at(index);
        }
    }

    pub fn change_all(&mut self, mut op: impl FnMut(&mut O::Item)) {
        let first_leaf = self.first_leaf();
        for item in self.data[first_leaf..].iter_mut().rev() {
            op(item);
        }
        for index in (0..first_leaf).rev() {
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
        // Build tree bottom to top in reversed heap order so that we can push
        // new nodes to the back of the vec.
        let mut data: Vec<_> = iter.into_iter().collect();
        let len = data.len();
        let tree_size = 2 * len - 1;
        data.resize_with(tree_size, Default::default);
        data.rotate_right(tree_size - len);

        for index in (0..(len - 1)).rev() {
            data[index] = O::combine(&data[left_child(index)], &data[right_child(index)]);
        }

        Self { data }
    }
}
