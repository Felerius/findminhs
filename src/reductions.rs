use crate::{
    data_structures::set_tries::{SubsetTrie, SupersetTrie},
    instance::{EdgeIdx, Instance, NodeIdx},
    lower_bound,
    small_indices::{IdxHashSet, SmallIdx},
};
use log::info;
use std::{cmp::Reverse, collections::BinaryHeap};

#[derive(Copy, Clone, Debug)]
enum ReducedItem {
    RemovedNode(NodeIdx),
    RemovedEdge(EdgeIdx),
    ForcedNode(NodeIdx),
}

impl ReducedItem {
    fn apply(self, instance: &mut Instance, partial_hs: &mut Vec<NodeIdx>) {
        match self {
            Self::RemovedNode(node_idx) => instance.delete_node(node_idx),
            Self::RemovedEdge(edge_idx) => instance.delete_edge(edge_idx),
            Self::ForcedNode(node_idx) => {
                instance.delete_node(node_idx);
                instance.delete_incident_edges(node_idx);
                partial_hs.push(node_idx);
            }
        }
    }

    fn restore(self, instance: &mut Instance, partial_hs: &mut Vec<NodeIdx>) {
        match self {
            Self::RemovedNode(node_idx) => instance.restore_node(node_idx),
            Self::RemovedEdge(edge_idx) => instance.restore_edge(edge_idx),
            Self::ForcedNode(node_idx) => {
                instance.restore_incident_edges(node_idx);
                instance.restore_node(node_idx);
                debug_assert_eq!(partial_hs.last().copied(), Some(node_idx));
                partial_hs.pop();
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Reduction(Vec<ReducedItem>);

impl Reduction {
    pub fn restore(&self, instance: &mut Instance, partial_hs: &mut Vec<NodeIdx>) {
        for item in self.0.iter().rev() {
            item.restore(instance, partial_hs)
        }
    }
}

#[derive(Debug, Clone)]
pub enum ReductionResult {
    Solved,
    Unsolvable,
    Finished,
}

fn find_dominated_nodes(instance: &Instance) -> impl Iterator<Item = ReducedItem> + '_ {
    let mut nodes = instance.nodes().to_vec();
    nodes.sort_unstable_by_key(|&node| Reverse(instance.node_degree(node)));
    let mut trie = SupersetTrie::new(instance.num_edges_total());
    nodes.into_iter().filter_map(move |node_idx| {
        if trie.contains_superset(instance.node(node_idx)) {
            Some(ReducedItem::RemovedNode(node_idx))
        } else {
            trie.insert(instance.node(node_idx));
            None
        }
    })
}

fn find_dominated_edges(instance: &Instance) -> impl Iterator<Item = ReducedItem> + '_ {
    let mut edges = instance.edges().to_vec();
    edges.sort_unstable_by_key(|&edge| instance.edge_degree(edge));
    let mut trie = SubsetTrie::new(instance.num_nodes_total());
    edges.into_iter().filter_map(move |edge_idx| {
        if trie.find_subset(instance.edge(edge_idx)) {
            Some(ReducedItem::RemovedEdge(edge_idx))
        } else {
            trie.insert(true, instance.edge(edge_idx));
            None
        }
    })
}

fn find_size_1_edges(instance: &Instance) -> impl Iterator<Item = ReducedItem> {
    let forced: IdxHashSet<_> = instance
        .edges()
        .iter()
        .copied()
        .filter_map(|edge_idx| {
            if instance.edge_degree(edge_idx) == 1 {
                Some(
                    instance
                        .edge(edge_idx)
                        .next()
                        .expect("Empty edge of size 1!?"),
                )
            } else {
                None
            }
        })
        .collect();
    forced.into_iter().map(ReducedItem::ForcedNode)
}

fn find_forced_choices<'a>(
    instance: &'a Instance,
    packing: &'a [EdgeIdx],
    remaining: &'a [EdgeIdx],
    partial_size: usize,
    smallest_known_size: usize,
) -> impl Iterator<Item = ReducedItem> + 'a {
    let mut blocked_by = vec![Vec::new(); instance.num_nodes_total()];
    let mut hit = vec![false; instance.num_nodes_total()];
    for &packed_edge in packing {
        for node_idx in instance.edge(packed_edge) {
            hit[node_idx.idx()] = true;
        }
    }

    for &remaining_edge in remaining {
        let mut blocking_nodes_iter = instance
            .edge(remaining_edge)
            .filter(|&node_idx| hit[node_idx.idx()]);
        let blocking_node = blocking_nodes_iter
            .next()
            .expect("Edge could have been added to packing");
        if blocking_nodes_iter.next().is_none() {
            blocked_by[blocking_node.idx()].push(remaining_edge);
        }
    }

    blocked_by
        .into_iter()
        .enumerate()
        .filter_map(move |(idx, mut blocked)| {
            let maybe_blocked_node = NodeIdx::from(idx);
            blocked.retain(|&edge_idx| {
                if instance
                    .edge(edge_idx)
                    .all(|node_idx| node_idx == maybe_blocked_node || !hit[node_idx.idx()])
                {
                    for node_idx in instance.edge(edge_idx) {
                        hit[node_idx.idx()] = true;
                    }
                    true
                } else {
                    false
                }
            });

            let new_lower_bound = partial_size + packing.len() + blocked.len();

            for edge_idx in blocked {
                for node_idx in instance.edge(edge_idx) {
                    hit[node_idx.idx()] = false;
                }
            }
            hit[maybe_blocked_node.idx()] = true;

            if new_lower_bound >= smallest_known_size {
                Some(ReducedItem::ForcedNode(maybe_blocked_node))
            } else {
                None
            }
        })
}

pub fn greedy_approx(instance: &Instance) -> Vec<NodeIdx> {
    let mut hit = vec![true; instance.num_edges_total()];
    for edge_idx in instance.edges() {
        hit[edge_idx.idx()] = false;
    }
    let mut node_degrees = vec![0; instance.num_nodes_total()];
    let mut node_queue = BinaryHeap::new();
    for &node_idx in instance.nodes() {
        node_degrees[node_idx.idx()] = instance.node_degree(node_idx);
        node_queue.push((node_degrees[node_idx.idx()], node_idx));
    }

    let mut hs = Vec::new();
    while let Some((degree, node_idx)) = node_queue.pop() {
        if degree == 0 {
            break;
        }
        if degree > node_degrees[node_idx.idx()] {
            continue;
        }

        hs.push(node_idx);
        node_degrees[node_idx.idx()] = 0; // Fewer elements in the heap
        for edge_idx in instance.node(node_idx) {
            if hit[edge_idx.idx()] {
                continue;
            }

            hit[edge_idx.idx()] = true;
            for edge_node_idx in instance.edge(edge_idx) {
                if node_degrees[edge_node_idx.idx()] > 0 {
                    node_degrees[edge_node_idx.idx()] -= 1;
                    node_queue.push((node_degrees[edge_node_idx.idx()], edge_node_idx));
                }
            }
        }
    }

    hs
}

pub fn reduce(
    instance: &mut Instance,
    partial_hs: &mut Vec<NodeIdx>,
    minimum_hs: &mut Vec<NodeIdx>,
) -> (ReductionResult, Reduction) {
    let mut reduced = Vec::new();
    let result = loop {
        let greedy = greedy_approx(instance);
        if partial_hs.len() + greedy.len() < minimum_hs.len() {
            minimum_hs.clear();
            minimum_hs.extend(partial_hs.iter().copied());
            minimum_hs.extend(greedy.iter().copied());
            info!(
                "Found HS of size {} using greedy (partial {} + greedy {})",
                minimum_hs.len(),
                partial_hs.len(),
                greedy.len()
            );
        }
        if partial_hs.len() >= minimum_hs.len() {
            break ReductionResult::Unsolvable;
        }

        match instance
            .edges()
            .iter()
            .map(|&edge_idx| instance.edge_degree(edge_idx))
            .min()
        {
            None => break ReductionResult::Solved,
            Some(0) => break ReductionResult::Unsolvable,
            Some(_) => {}
        }

        let (packing, remaining) = lower_bound::pack_edges(instance);
        if lower_bound::calculate(instance, &packing, partial_hs.len()) >= minimum_hs.len() {
            break ReductionResult::Unsolvable;
        }

        let len_before = reduced.len();
        reduced.extend(find_size_1_edges(instance));

        if reduced.len() == len_before {
            reduced.extend(find_forced_choices(
                instance,
                &packing,
                &remaining,
                partial_hs.len(),
                minimum_hs.len(),
            ));
        }

        if reduced.len() == len_before {
            reduced.extend(find_dominated_nodes(instance));
        }

        if reduced.len() == len_before {
            reduced.extend(find_dominated_edges(instance));
        }

        if reduced.len() == len_before {
            break ReductionResult::Finished;
        }

        for reduced_item in &reduced[len_before..] {
            reduced_item.apply(instance, partial_hs);
        }
    };

    (result, Reduction(reduced))
}
