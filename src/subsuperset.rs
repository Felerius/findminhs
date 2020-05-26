use crate::create_idx_struct;
use crate::data_structures::skipvec::SkipVec;
use crate::instance::{EdgeIdx, EntryIdx, Instance, NodeIdx};
use crate::small_indices::{IdxHashMap, SmallIdx};
use crate::solve::Stats;
use log::{debug, log_enabled, trace, Level};
use std::cmp::Reverse;
use std::collections::btree_map::Range as BTreeMapRange;
use std::collections::BTreeMap;
use std::mem;
use std::ops::Bound;
use std::time::Instant;

#[derive(Copy, Clone, Debug)]
enum ReducedItem {
    Node(NodeIdx),
    Edge(EdgeIdx),
}

#[derive(Clone, Debug, Default)]
pub struct Reduction {
    reduced: Vec<ReducedItem>,
}

#[derive(Debug)]
enum SmallMap<K: SmallIdx, V> {
    Small(Vec<V>),
    Large(IdxHashMap<K, V>),
}

create_idx_struct!(TrieNodeIdx);
create_idx_struct!(SetIdx);

#[derive(Debug)]
struct SubsetTrie {
    num_nodes: usize,
    nexts: Vec<SmallMap<NodeIdx, TrieNodeIdx>>,
    is_set: Vec<bool>,
    stack: Vec<(TrieNodeIdx, SetIdx)>,
}

#[derive(Debug)]
struct SupersetTrie {
    nexts: Vec<BTreeMap<EdgeIdx, TrieNodeIdx>>,
    is_set: Vec<bool>,
    stack: Vec<(
        TrieNodeIdx,
        SetIdx,
        BTreeMapRange<'static, EdgeIdx, TrieNodeIdx>,
    )>,
}

impl<K: SmallIdx, V: SmallIdx> SmallMap<K, V> {
    fn new(key_range_size: usize) -> Self {
        if key_range_size < 256 {
            Self::Small(vec![V::INVALID; key_range_size])
        } else {
            Self::Large(IdxHashMap::default())
        }
    }

    fn get(&self, key: K) -> V {
        match self {
            Self::Small(vec) => vec[key.idx()],
            Self::Large(map) => map.get(&key).copied().unwrap_or(V::INVALID),
        }
    }

    fn get_or_insert(&mut self, key: K, value: V) -> V {
        match self {
            Self::Small(vec) => {
                if !vec[key.idx()].valid() {
                    vec[key.idx()] = value;
                }
                vec[key.idx()]
            }
            Self::Large(map) => *map.entry(key).or_insert(value),
        }
    }
}

impl SubsetTrie {
    fn new(num_nodes: usize) -> Self {
        Self {
            num_nodes,
            nexts: vec![SmallMap::new(num_nodes)],
            is_set: vec![false],
            stack: Vec::with_capacity(num_nodes),
        }
    }

    fn insert(&mut self, iter: impl IntoIterator<Item = NodeIdx>) {
        let mut idx = TrieNodeIdx(0);
        for node_idx in iter {
            let new_node_idx = TrieNodeIdx::from(self.nexts.len());
            idx = self.nexts[idx.idx()].get_or_insert(node_idx, new_node_idx);
            if idx == new_node_idx {
                self.nexts.push(SmallMap::new(self.num_nodes));
                self.is_set.push(false);
            }
        }
        self.is_set[idx.idx()] = true;
    }

    fn contains_subset(&mut self, set: &SkipVec<(NodeIdx, EntryIdx)>) -> bool {
        if self.is_set[0] {
            return true;
        }

        let first_idx = if let Some(idx) = set.first() {
            SetIdx::from(idx)
        } else {
            return false;
        };

        debug_assert!(self.stack.is_empty());
        self.stack.push((TrieNodeIdx(0), first_idx));
        while let Some((trie_node, mut set_idx)) = self.stack.pop() {
            let nexts = &self.nexts[trie_node.idx()];
            loop {
                let item = set[set_idx.idx()].0;
                let next_node = nexts.get(item);
                set_idx = set
                    .next(set_idx.idx())
                    .map_or(SetIdx::INVALID, SetIdx::from);
                if next_node.valid() {
                    if self.is_set[next_node.idx()] {
                        self.stack.clear();
                        return true;
                    }
                    if set_idx.valid() {
                        self.stack.push((trie_node, set_idx));
                        self.stack.push((next_node, set_idx));
                        break;
                    }
                }
                if !set_idx.valid() {
                    break;
                }
            }
        }
        false
    }
}

impl SupersetTrie {
    fn new(num_edges: usize) -> Self {
        Self {
            nexts: vec![BTreeMap::new()],
            is_set: vec![false],
            stack: Vec::with_capacity(num_edges),
        }
    }

    fn insert(&mut self, iter: impl IntoIterator<Item = EdgeIdx>) {
        let mut idx = TrieNodeIdx(0);
        for item in iter {
            let new_node_idx = TrieNodeIdx::from(self.nexts.len());
            idx = *self.nexts[idx.idx()].entry(item).or_insert(new_node_idx);
            if idx == new_node_idx {
                self.nexts.push(BTreeMap::new());
                self.is_set.push(false);
            }
        }
        self.is_set[idx.idx()] = true;
    }

    fn contains_superset(&mut self, set: &SkipVec<(EdgeIdx, EntryIdx)>) -> bool {
        let first_idx = if let Some(idx) = set.first() {
            SetIdx::from(idx)
        } else {
            return true;
        };

        let mut stack = mem::take(&mut self.stack);
        let edge_zero = EdgeIdx::from(0_u32);
        let first_edge = set[first_idx.idx()].0;
        stack.push((
            TrieNodeIdx(0),
            first_idx,
            self.nexts[0].range(edge_zero..=first_edge),
        ));

        let mut result = false;
        while let Some((trie_idx, set_idx, mut range)) = stack.pop() {
            // Iterate the range backwards, so that if we have a match for the
            // next item from the set, we process it first.
            if let Some((&edge_idx, &next_trie_idx)) = range.next_back() {
                let cur_item = set[set_idx.idx()].0;
                stack.push((trie_idx, set_idx, range));
                if edge_idx == cur_item {
                    if let Some(next_set_idx) = set.next(set_idx.idx()) {
                        let next_item = set[next_set_idx].0;
                        let next_range = self.nexts[next_trie_idx.idx()]
                            .range((Bound::Excluded(cur_item), Bound::Included(next_item)));
                        stack.push((next_trie_idx, SetIdx::from(next_set_idx), next_range));
                    } else {
                        result = true;
                        break;
                    }
                } else {
                    let lower_range_bound = set
                        .prev(set_idx.idx())
                        .map_or(Bound::Included(edge_zero), |prev_idx| {
                            Bound::Excluded(set[prev_idx].0)
                        });
                    let next_range = self.nexts[next_trie_idx.idx()]
                        .range((lower_range_bound, Bound::Included(cur_item)));
                    stack.push((next_trie_idx, set_idx, next_range));
                }
            }
        }

        // Cast the stack back to one with 'static ranges. This is safe
        // changing the lifetime does not change the size of the range type
        // (otherwise the cast the other way around wouldn't be safe), and we
        // never read from what remains on the stack since we set the length to
        // zero.
        stack.clear();
        let stack_ptr = stack.as_mut_ptr();
        let capacity = stack.capacity();
        // Leak stack
        mem::ManuallyDrop::new(stack);
        // ptr-ptr casts don't like casting lifetime params
        // (https://github.com/rust-lang/rust/issues/27214), so we need to
        // cast with one in-between stop
        let void_ptr = stack_ptr as *mut u64;
        self.stack = unsafe { Vec::from_raw_parts(void_ptr as *mut _, 0, capacity) };

        result
    }
}

impl Reduction {
    pub fn nodes(&self) -> impl Iterator<Item = NodeIdx> + '_ {
        self.reduced.iter().filter_map(|item| match item {
            ReducedItem::Node(node_idx) => Some(*node_idx),
            ReducedItem::Edge(_) => None,
        })
    }

    pub fn restore(&self, instance: &mut Instance) {
        for &item in self.reduced.iter().rev() {
            match item {
                ReducedItem::Node(node_idx) => instance.restore_node(node_idx),
                ReducedItem::Edge(edge_idx) => instance.restore_edge(edge_idx),
            }
        }
    }
}

fn prune_redundant_nodes(instance: &mut Instance, reduction: &mut Reduction) -> usize {
    let mut nodes = instance.nodes().to_vec();
    nodes.sort_unstable_by_key(|&node| Reverse(instance.node_degree(node)));

    let mut trie = SupersetTrie::new(instance.num_edges_total());
    let mut num_kept = 0;
    for idx in 0..nodes.len() {
        let node = nodes[idx];
        if trie.contains_superset(instance.node_vec(node)) {
            trace!("Pruning node {}", node);
            instance.delete_node(node);
            reduction.reduced.push(ReducedItem::Node(node));
        } else {
            trie.insert(instance.node(node));
            num_kept += 1;
        }

        if log_enabled!(Level::Debug) && (idx + 1) % 1000 == 0 {
            debug!(
                "Pruning nodes: {}/{} ({} kept)",
                idx + 1,
                nodes.len(),
                num_kept
            );
        }
    }
    nodes.len() - num_kept
}

fn prune_redundant_edges(instance: &mut Instance, reduction: &mut Reduction) -> usize {
    let mut edges = instance.edges().to_vec();
    edges.sort_unstable_by_key(|&edge| instance.edge_degree(edge));

    let mut trie = SubsetTrie::new(instance.num_nodes_total());
    let mut num_kept = 0;
    for idx in 0..edges.len() {
        let edge = edges[idx];
        if trie.contains_subset(instance.edge_vec(edge)) {
            trace!("Pruning edge {}", edge);
            instance.delete_edge(edge);
            reduction.reduced.push(ReducedItem::Edge(edge));
        } else {
            trie.insert(instance.edge(edge));
            num_kept += 1;
        }

        if log_enabled!(Level::Debug) && (idx + 1) % 1000 == 0 {
            debug!(
                "Pruning edges: {}/{} ({} kept)",
                idx + 1,
                edges.len(),
                num_kept
            );
        }
    }
    edges.len() - num_kept
}

pub fn prune(instance: &mut Instance, stats: &mut Stats) -> Reduction {
    let time_start = Instant::now();
    let mut reduction = Reduction::default();
    let mut pruned_nodes = 0;
    let mut pruned_edges = 0;
    let mut current_iter = 0;
    loop {
        current_iter += 1;
        let time_start_iteration = Instant::now();
        let iter_pruned_nodes = prune_redundant_nodes(instance, &mut reduction);
        let iter_pruned_edges = prune_redundant_edges(instance, &mut reduction);
        trace!(
            "Iteration {}: pruned {} nodes, {} edges in {:.2?}",
            current_iter,
            iter_pruned_nodes,
            iter_pruned_edges,
            Instant::now() - time_start_iteration
        );
        pruned_nodes += iter_pruned_nodes;
        pruned_edges += iter_pruned_edges;
        if iter_pruned_nodes == 0 && iter_pruned_edges == 0 {
            break;
        }
    }
    let elapsed = Instant::now() - time_start;
    stats.subsuper_prune_time += elapsed;
    debug!(
        "Pruned {} nodes, {} edges in {} iterations ({:.2?}), remaining: {} nodes, {} edges",
        pruned_nodes,
        pruned_edges,
        current_iter,
        elapsed,
        instance.num_nodes(),
        instance.num_edges(),
    );
    reduction
}
