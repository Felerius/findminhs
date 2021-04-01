use crate::data_structures::set_tries::{SubsetTrie, SupersetTrie};
use crate::instance::{EdgeIdx, Instance, NodeIdx};
use crate::small_indices::SmallIdx;
use log::info;
use std::cmp::Reverse;

#[derive(Copy, Clone, Debug)]
pub enum ReducedItem {
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

#[derive(Debug, Clone)]
pub enum LowerBoundResult {
    PruneBranch,
    ForcedNodes(Vec<ReducedItem>),
}

fn find_dominated_nodes(instance: &Instance) -> impl Iterator<Item = ReducedItem> + '_ {
    let mut nodes = instance.nodes().to_vec();
    nodes.sort_unstable_by_key(|&node| Reverse(instance.node_degree(node)));
    let mut trie = SupersetTrie::new(instance.num_edges_total());
    nodes.into_iter().filter_map(move |node_idx| {
        if trie.contains_superset(instance.node_vec(node_idx)) {
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
        if trie.contains_subset(instance.edge_vec(edge_idx)) {
            Some(ReducedItem::RemovedEdge(edge_idx))
        } else {
            trie.insert(instance.edge(edge_idx));
            None
        }
    })
}

fn find_size_1_edge(instance: &Instance) -> Option<ReducedItem> {
    instance.min_edge_degree().and_then(|(degree, edge_idx)| {
        if degree == 1 {
            let node_idx = instance
                .edge(edge_idx)
                .next()
                .expect("Degree 1 edge is empty");
            Some(ReducedItem::ForcedNode(node_idx))
        } else {
            None
        }
    })
}

pub fn lower_bound(
    instance: &Instance,
    partial_size: usize,
    smallest_known_size: usize,
) -> (usize, LowerBoundResult) {
    let mut edges: Vec<_> = instance.edges().iter().copied().collect();
    edges.sort_by_cached_key(|&edge_idx| {
        instance
            .edge(edge_idx)
            .map(|node_idx| instance.node_degree(node_idx))
            .sum::<usize>()
    });

    let mut hit = vec![false; instance.num_nodes_total()];
    let mut lower_bound = partial_size;
    let mut blocked_by = vec![vec![]; instance.num_nodes_total()];
    for edge_idx in edges {
        let mut blocking_iter = instance
            .edge(edge_idx)
            .filter(|node_idx| hit[node_idx.idx()]);
        if let Some(first_blocking) = blocking_iter.next() {
            if blocking_iter.next().is_none() {
                blocked_by[first_blocking.idx()].push(edge_idx);
            }
        } else {
            lower_bound += 1;
            for node_idx in instance.edge(edge_idx) {
                hit[node_idx.idx()] = true;
            }
        }
    }

    if lower_bound >= smallest_known_size {
        return (lower_bound, LowerBoundResult::PruneBranch);
    }

    let mut undo_stack = vec![];
    let forced_nodes = instance
        .nodes()
        .iter()
        .copied()
        .filter_map(|node_idx| {
            let mut new_lower_bound = lower_bound;
            for &edge_idx in &blocked_by[node_idx.idx()] {
                if instance
                    .edge(edge_idx)
                    .all(|edge_node| edge_node == node_idx || !hit[edge_node.idx()])
                {
                    new_lower_bound += 1;
                    for edge_node in instance.edge(edge_idx) {
                        if edge_node != node_idx {
                            hit[edge_node.idx()] = true;
                            undo_stack.push(edge_node);
                        }
                    }
                }
            }

            for undo_node in undo_stack.drain(..) {
                hit[undo_node.idx()] = false;
            }

            if new_lower_bound >= smallest_known_size {
                Some(ReducedItem::ForcedNode(node_idx))
            } else {
                None
            }
        })
        .collect();

    (lower_bound, LowerBoundResult::ForcedNodes(forced_nodes))
}

pub fn greedy_approx(instance: &mut Instance) -> Vec<NodeIdx> {
    let mut hs = Vec::new();
    while !instance.edges().is_empty() {
        let max_degree_node = instance.max_node_degree().1;
        instance.delete_node(max_degree_node);
        instance.delete_incident_edges(max_degree_node);
        hs.push(max_degree_node);
    }
    for &node in hs.iter().rev() {
        instance.restore_incident_edges(node);
        instance.restore_node(node);
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
        match instance.min_edge_degree() {
            None => break ReductionResult::Solved,
            Some((0, _)) => break ReductionResult::Unsolvable,
            Some(_) => {}
        }

        if let Some(forced_node) = find_size_1_edge(instance) {
            forced_node.apply(instance, partial_hs);
            reduced.push(forced_node);
            continue;
        }

        match lower_bound(instance, partial_hs.len(), minimum_hs.len()).1 {
            LowerBoundResult::PruneBranch => break ReductionResult::Unsolvable,
            LowerBoundResult::ForcedNodes(forced_nodes) => {
                if !forced_nodes.is_empty() {
                    for forced_node in forced_nodes {
                        forced_node.apply(instance, partial_hs);
                        reduced.push(forced_node);
                    }
                    continue;
                }
            }
        }

        let mut len_before = reduced.len();
        reduced.extend(find_dominated_nodes(instance));
        if reduced.len() > len_before {
            for reduced_item in &reduced[len_before..] {
                reduced_item.apply(instance, partial_hs);
            }
            continue;
        }

        len_before = reduced.len();
        reduced.extend(find_dominated_edges(instance));
        if reduced.len() > len_before {
            for reduced_item in &reduced[len_before..] {
                reduced_item.apply(instance, partial_hs);
            }
            continue;
        }

        break ReductionResult::Finished;
    };

    (result, Reduction(reduced))
}
