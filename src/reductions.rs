use crate::instance::{EdgeIdx, Instance, NodeIdx};
use log::{debug, log_enabled, trace, Level};
use std::cmp::Reverse;
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

impl Reduction {
    pub fn nodes(&self) -> impl Iterator<Item=NodeIdx> + '_ {
        self.reduced.iter().filter_map(|item| {
            match item {
                ReducedItem::Node(node_idx) => Some(*node_idx),
                ReducedItem::Edge(_) => None,
            }
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

fn is_subset_or_equal<T, I1, I2>(left: I1, right: I2) -> bool
where
    I1: IntoIterator<Item = T>,
    I2: IntoIterator<Item = T>,
    T: Ord,
{
    let mut iter_right = right.into_iter().peekable();
    for item_left in left {
        while let Some(item_right) = iter_right.peek() {
            if item_right >= &item_left {
                break;
            }
            iter_right.next();
        }
        match iter_right.next() {
            None => return false,
            Some(item_right) if item_left != item_right => return false,
            _ => {}
        }
    }
    true
}

fn prune_redundant_nodes(instance: &mut Instance, reduction: &mut Reduction) -> usize {
    let mut nodes = instance.nodes().to_vec();
    nodes.sort_unstable_by_key(|&node| Reverse(instance.node_degree(node)));

    let mut num_kept = 0;
    for idx in 0..nodes.len() {
        let node = nodes[idx];
        let mut prunable = false;
        for &larger_node in &nodes[0..num_kept] {
            if is_subset_or_equal(instance.node(node), instance.node(larger_node)) {
                trace!("Deleting node {} because of {}", node, larger_node);
                prunable = true;
                break;
            }
        }
        if prunable {
            instance.delete_node(node);
            reduction.reduced.push(ReducedItem::Node(node));
        } else {
            nodes.swap(num_kept, idx);
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

    let mut num_kept = 0;
    for idx in 0..edges.len() {
        let edge = edges[idx];
        let mut prunable = false;
        for &smaller_edge in &edges[0..num_kept] {
            if is_subset_or_equal(instance.edge(smaller_edge), instance.edge(edge)) {
                trace!("Deleting edge {} because of {}", edge, smaller_edge);
                prunable = true;
                break;
            }
        }
        if prunable {
            instance.delete_edge(edge);
            reduction.reduced.push(ReducedItem::Edge(edge));
        } else {
            edges.swap(num_kept, idx);
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

pub fn prune(instance: &mut Instance) -> Reduction {
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
    debug!(
        "Pruned {} nodes, {} edges in {} iterations ({:.2?}), remaining: {} nodes, {} edges",
        pruned_nodes,
        pruned_edges,
        current_iter,
        Instant::now() - time_start,
        instance.num_nodes(),
        instance.num_edges(),
    );
    reduction
}

#[cfg(test)]
mod tests {
    use super::is_subset_or_equal;

    #[test]
    fn test_is_subset_or_equal() {
        assert!(is_subset_or_equal(vec![1], vec![1, 2]));
        assert!(is_subset_or_equal(vec![1, 2], vec![1, 2]));
        assert!(!is_subset_or_equal(vec![1, 3], vec![1, 2]));
        assert!(!is_subset_or_equal(vec![1, 2, 3], vec![1, 2]));
    }
}
