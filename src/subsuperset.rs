use crate::create_idx_struct;
use crate::instance::{EdgeIdx, Instance, NodeIdx};
use crate::small_indices::SmallIdx;
use crate::solve::Stats;
use fxhash::FxHasher32;
use log::{debug, log_enabled, trace, Level};
use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap};
use std::hash::{BuildHasherDefault, Hash};
use std::iter::Peekable;
use std::time::Instant;

#[derive(Debug)]
enum ReducedItem {
    Node(NodeIdx),
    Edge(EdgeIdx),
}

#[derive(Debug, Default)]
pub struct Reduction {
    reduced: Vec<ReducedItem>,
}

type Fx32HashMap<K, V> = HashMap<K, V, BuildHasherDefault<FxHasher32>>;

#[derive(Debug)]
enum SmallMap<K: SmallIdx, V> {
    Small(Vec<V>),
    Large(Fx32HashMap<K, V>),
}

create_idx_struct!(TrieNodeIdx);

#[derive(Debug)]
struct SubsetTrie<T: SmallIdx> {
    item_range_len: usize,
    nexts: Vec<SmallMap<T, TrieNodeIdx>>,
    is_set: Vec<bool>,
}

#[derive(Debug)]
struct SupersetTrie<T: SmallIdx> {
    nexts: Vec<BTreeMap<T, TrieNodeIdx>>,
    is_set: Vec<bool>,
}

impl<K: SmallIdx, V: SmallIdx> SmallMap<K, V> {
    fn new(key_range_size: usize) -> Self {
        if key_range_size < 256 {
            Self::Small(vec![V::INVALID; key_range_size])
        } else {
            Self::Large(Fx32HashMap::default())
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

impl<T: SmallIdx> SubsetTrie<T> {
    fn new(item_range_len: usize) -> Self {
        Self {
            item_range_len,
            nexts: vec![SmallMap::new(item_range_len)],
            is_set: vec![false],
        }
    }

    fn insert(&mut self, iter: impl IntoIterator<Item = T>) {
        let mut idx = TrieNodeIdx(0);
        for item in iter {
            let new_node_idx = TrieNodeIdx::from(self.nexts.len());
            idx = self.nexts[idx.idx()].get_or_insert(item, new_node_idx);
            if idx == new_node_idx {
                self.nexts.push(SmallMap::new(self.item_range_len));
                self.is_set.push(false);
            }
        }
        self.is_set[idx.idx()] = true;
    }

    fn contains_subset_at(
        &self,
        node: TrieNodeIdx,
        mut iter: impl Iterator<Item = T> + Clone,
    ) -> bool {
        let next = &self.nexts[node.idx()];
        while let Some(item) = iter.next() {
            let next_node = next.get(item);
            if next_node.valid() {
                if self.is_set[next_node.idx()] || self.contains_subset_at(next_node, iter.clone())
                {
                    return true;
                }
            }
        }
        false
    }

    fn contains_subset<I>(&self, iter: I) -> bool
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: Clone,
    {
        self.is_set[0] || self.contains_subset_at(TrieNodeIdx(0), iter.into_iter())
    }
}

impl<T: SmallIdx> SupersetTrie<T> {
    fn new() -> Self {
        Self {
            nexts: vec![BTreeMap::new()],
            is_set: vec![false],
        }
    }

    fn insert(&mut self, iter: impl IntoIterator<Item = T>) {
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

    fn contains_superset_at(
        &self,
        node: TrieNodeIdx,
        lower: T,
        mut iter: Peekable<impl Iterator<Item = T> + Clone>,
    ) -> bool {
        let next = &self.nexts[node.idx()];
        let upper = if let Some(&upper) = iter.peek() {
            upper
        } else {
            return true;
        };
        for (&value, &next_node) in next.range(lower..=upper) {
            let mut iter_clone = iter.clone();
            let result = if value == upper {
                iter_clone.next();
                self.contains_superset_at(next_node, upper, iter_clone)
            } else {
                self.contains_superset_at(next_node, lower, iter_clone)
            };
            if result {
                return true;
            }
        }
        false
    }

    fn contains_superset<I>(&self, iter: I) -> bool
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: Clone,
    {
        self.contains_superset_at(TrieNodeIdx(0), 0usize.into(), iter.into_iter().peekable())
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
        for item in self.reduced.iter().rev() {
            match item {
                ReducedItem::Node(node_idx) => instance.restore_node(*node_idx),
                ReducedItem::Edge(edge_idx) => instance.restore_edge(*edge_idx),
            }
        }
    }
}

fn prune_redundant_nodes(instance: &mut Instance, reduction: &mut Reduction) -> usize {
    let mut nodes = instance.nodes().to_vec();
    nodes.sort_unstable_by_key(|&node| Reverse(instance.node_degree(node)));

    let mut trie = SupersetTrie::new();
    let mut num_kept = 0;
    for idx in 0..nodes.len() {
        let node = nodes[idx];
        if trie.contains_superset(instance.node(node)) {
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
        if trie.contains_subset(instance.edge(edge)) {
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
