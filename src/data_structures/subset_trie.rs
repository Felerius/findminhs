use crate::{
    create_idx_struct,
    small_indices::{IdxHashMap, SmallIdx},
};
use std::collections::hash_map::Entry;

create_idx_struct!(TrieNodeIdx);

#[derive(Debug)]
enum SubsetTrieChildren<I> {
    Small(usize, Vec<TrieNodeIdx>),
    Large(Vec<IdxHashMap<I, TrieNodeIdx>>),
}

impl<V: SmallIdx> SubsetTrieChildren<V> {
    fn new(key_range: usize) -> Self {
        if key_range <= 32 {
            Self::Small(key_range, vec![TrieNodeIdx::INVALID; key_range])
        } else {
            Self::Large(vec![IdxHashMap::default()])
        }
    }

    fn get(&self, node: TrieNodeIdx, edge_val: V) -> TrieNodeIdx {
        match *self {
            Self::Small(key_range, ref flat) => flat[node.idx() * key_range + edge_val.idx()],
            Self::Large(ref maps) => maps[node.idx()].get(&edge_val).copied().unwrap_or_default(),
        }
    }

    fn get_or_insert(&mut self, node: TrieNodeIdx, edge_val: V) -> (TrieNodeIdx, bool) {
        match *self {
            Self::Small(key_range, ref mut flat) => {
                let idx = node.idx() * key_range + edge_val.idx();
                if flat[idx].valid() {
                    (flat[idx], false)
                } else {
                    flat[idx] = TrieNodeIdx::from(flat.len() / key_range);
                    flat.resize(flat.len() + key_range, TrieNodeIdx::INVALID);
                    (flat[idx], true)
                }
            }
            Self::Large(ref mut maps) => {
                let new_node_idx = TrieNodeIdx::from(maps.len());
                match maps[node.idx()].entry(edge_val) {
                    Entry::Occupied(occupied) => (*occupied.get(), false),
                    Entry::Vacant(vacant) => {
                        vacant.insert(new_node_idx);
                        maps.push(IdxHashMap::default());
                        (new_node_idx, true)
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct SubsetTrie<V, M, I> {
    children: SubsetTrieChildren<V>,
    markers: Vec<M>,
    stack: Vec<(TrieNodeIdx, I)>,
}

impl<V, M, I> SubsetTrie<V, M, I>
where
    V: SmallIdx,
    M: Copy + Default + Eq,
    I: Iterator<Item = V> + Clone,
{
    pub fn new(key_range: usize) -> Self {
        Self {
            children: SubsetTrieChildren::new(key_range),
            markers: vec![M::default(); 1],
            stack: Vec::with_capacity(key_range),
        }
    }

    pub fn insert(&mut self, marker: M, set: impl IntoIterator<Item = V>) {
        let mut idx = TrieNodeIdx(0);
        for edge_val in set {
            let (new_idx, inserted) = self.children.get_or_insert(idx, edge_val);
            if inserted {
                self.markers.push(M::default());
            }
            idx = new_idx;
        }
        self.markers[idx.idx()] = marker;
    }

    pub fn find_subset(&mut self, iter: impl IntoIterator<IntoIter = I>) -> M {
        debug_assert!(self.stack.is_empty());
        self.stack.push((TrieNodeIdx(0), iter.into_iter()));
        while let Some((node, mut iter)) = self.stack.pop() {
            if self.markers[node.idx()] != M::default() {
                self.stack.clear();
                return self.markers[node.idx()];
            }

            while let Some(edge_val) = iter.next() {
                let next_node = self.children.get(node, edge_val);
                if next_node.valid() {
                    let iter_clone = iter.clone();
                    self.stack.push((node, iter));
                    self.stack.push((next_node, iter_clone));
                    break;
                }
            }
        }

        M::default()
    }
}
