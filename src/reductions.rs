use crate::data_structures::set_tries::{SubsetTrie, SupersetTrie};
use crate::instance::{EdgeIdx, Instance, NodeIdx};
use crate::solve::Stats;
use std::{cmp::Reverse, time::Instant};

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
    pub fn nodes(&self) -> impl Iterator<Item = NodeIdx> + '_ {
        self.0.iter().filter_map(|&item| match item {
            ReducedItem::RemovedNode(node_idx) | ReducedItem::ForcedNode(node_idx) => {
                Some(node_idx)
            }
            _ => None,
        })
    }

    pub fn restore(&self, instance: &mut Instance, partial_hs: &mut Vec<NodeIdx>) {
        for item in self.0.iter().rev() {
            item.restore(instance, partial_hs)
        }
    }
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

fn find_forced_node(instance: &Instance) -> Option<ReducedItem> {
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

pub fn reduce(
    instance: &mut Instance,
    partial_hs: &mut Vec<NodeIdx>,
    stats: &mut Stats,
    mut should_stop_early: impl FnMut(&Instance, &[NodeIdx]) -> bool,
) -> Reduction {
    let time_start = Instant::now();
    let mut reduced = Vec::new();

    loop {
        let len_start = reduced.len();
        if should_stop_early(instance, partial_hs) {
            break;
        }

        reduced.extend(find_dominated_nodes(instance));
        for &item in &reduced[len_start..] {
            item.apply(instance, partial_hs);
        }
        if should_stop_early(instance, partial_hs) {
            break;
        }

        let len_middle = reduced.len();
        reduced.extend(find_dominated_edges(instance));
        for &item in &reduced[len_middle..] {
            item.apply(instance, partial_hs);
        }
        if should_stop_early(instance, partial_hs) {
            break;
        }

        if let Some(item) = find_forced_node(instance) {
            item.apply(instance, partial_hs);
            reduced.push(item);
        }

        if reduced.len() == len_start {
            break;
        }
    }

    let elapsed = Instant::now() - time_start;
    stats.reduction_time += elapsed;
    Reduction(reduced)
}
