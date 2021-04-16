use crate::{create_idx_struct, small_indices::SmallIdx};
use std::{collections::BTreeMap, iter::Peekable, ops::Range};

create_idx_struct!(TrieNodeIdx);

pub struct SupersetTrie<V: 'static, I: Iterator> {
    children: Vec<BTreeMap<V, TrieNodeIdx>>,
    is_set: Vec<bool>,
    stack: Vec<(TrieNodeIdx, Peekable<I>, Range<V>)>,
}

fn range_incl_incl<I: SmallIdx>(start: I, end: I) -> Range<I> {
    start..I::from(end.idx() + 1)
}

fn range_excl_incl<I: SmallIdx>(start: I, end: I) -> Range<I> {
    I::from(start.idx() + 1)..end
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

    pub fn contains_superset(&mut self, set: impl IntoIterator<IntoIter = I>) -> bool {
        let edge_val_zero = V::from(0_u32);
        let mut iter = set.into_iter().peekable();
        if let Some(&first_val) = iter.peek() {
            self.stack.push((
                TrieNodeIdx(0),
                iter,
                range_incl_incl(edge_val_zero, first_val),
            ));
        } else {
            // Any non-empty trie contains a leaf.
            return self.children.len() > 1;
        }

        while let Some((node, mut iter, range)) = self.stack.pop() {
            let val_to_match = *iter
                .peek()
                .expect("Empty iterator should not have been pushed on stack");
            let range_start = range.start;

            // Iterate the range backwards, so that if we have a match for the
            // next item from the set, we process it first.
            if let Some((&edge_val, &next_node)) =
                self.children[node.idx()].range(range).next_back()
            {
                self.stack.push((node, iter.clone(), range_start..edge_val));
                if edge_val == val_to_match {
                    iter.next();
                    if let Some(&next_val_to_match) = iter.peek() {
                        let next_range = range_excl_incl(val_to_match, next_val_to_match);
                        self.stack.push((next_node, iter, next_range));
                    } else {
                        // We would have moved below the root, so the trie is non-empty and there
                        // is a leaf below
                        return true;
                    }
                } else {
                    let next_range = range_excl_incl(edge_val, val_to_match);
                    self.stack.push((next_node, iter, next_range));
                }
            }
        }

        false
    }
}
