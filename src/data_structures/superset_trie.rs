use crate::{create_idx_struct, small_indices::SmallIdx};
use std::{
    collections::{btree_map::Range as BTreeMapRange, BTreeMap},
    iter::Peekable,
    mem,
    ops::Bound,
};

create_idx_struct!(TrieNodeIdx);

pub struct SupersetTrie<V: 'static, I: Iterator> {
    children: Vec<BTreeMap<V, TrieNodeIdx>>,
    is_set: Vec<bool>,
    stack: Vec<(
        TrieNodeIdx,
        Peekable<I>,
        BTreeMapRange<'static, V, TrieNodeIdx>,
    )>,
}

impl<V, I> SupersetTrie<V, I>
where
    V: SmallIdx,
    I: Iterator<Item = V> + Clone,
{
    pub fn new(val_range: usize) -> Self {
        Self {
            children: vec![BTreeMap::new()],
            is_set: vec![false],
            stack: Vec::with_capacity(val_range),
        }
    }

    pub fn insert(&mut self, iter: impl IntoIterator<Item = V>) {
        let mut idx = TrieNodeIdx(0);
        for item in iter {
            let new_node_idx = TrieNodeIdx::from(self.children.len());
            idx = *self.children[idx.idx()].entry(item).or_insert(new_node_idx);
            if idx == new_node_idx {
                self.children.push(BTreeMap::new());
                self.is_set.push(false);
            }
        }
        self.is_set[idx.idx()] = true;
    }

    fn contains_superset_with_stack<'a>(
        &'a self,
        set: I,
        stack: &mut Vec<(TrieNodeIdx, Peekable<I>, BTreeMapRange<'a, V, TrieNodeIdx>)>,
    ) -> bool {
        let edge_val_zero = V::from(0_u32);
        let mut iter = set.peekable();
        if let Some(&first_val) = iter.peek() {
            stack.push((
                TrieNodeIdx(0),
                iter,
                self.children[0].range(edge_val_zero..=first_val),
            ));
        } else {
            // Any non-empty trie contains a leaf.
            return self.children.len() > 1;
        }

        while let Some((node, mut iter, mut range)) = stack.pop() {
            let val_to_match = *iter
                .peek()
                .expect("Empty iterator should not have been pushed on stack");

            // Iterate the range backwards, so that if we have a match for the
            // next item from the set, we process it first.
            if let Some((&edge_val, &next_node)) = range.next_back() {
                stack.push((node, iter.clone(), range));
                if edge_val == val_to_match {
                    iter.next();
                    if let Some(&next_val_to_match) = iter.peek() {
                        let next_range = self.children[next_node.idx()].range((
                            Bound::Excluded(val_to_match),
                            Bound::Included(next_val_to_match),
                        ));
                        stack.push((next_node, iter, next_range));
                    } else {
                        // We would have moved below the root, so the trie is non-empty and there
                        // is a leaf below
                        return true;
                    }
                } else {
                    let next_range = self.children[next_node.idx()]
                        .range((Bound::Excluded(edge_val), Bound::Included(val_to_match)));
                    stack.push((next_node, iter, next_range));
                }
            }
        }

        false
    }

    pub fn contains_superset(&mut self, set: impl IntoIterator<IntoIter = I>) -> bool {
        let mut stack = mem::take(&mut self.stack);
        let result = self.contains_superset_with_stack(set.into_iter(), &mut stack);

        stack.clear();
        let ptr = stack.as_mut_ptr();
        let cap = stack.capacity();
        mem::forget(stack);
        self.stack = unsafe { Vec::from_raw_parts(ptr.cast(), 0, cap) };

        result
    }
}
