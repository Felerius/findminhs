use crate::{
    instance::{EdgeIdx, Instance, NodeIdx},
    small_indices::SmallIdx,
};

pub struct LowerBound {
    /// Lower bound found by a greedy independent set of edges increased by a bound on the number
    /// of nodes to cover remaining edges.
    pub lower_bound: usize,

    /// Lower bound only from greedy independent set of edges.
    pub independent_set_only_bound: usize,

    /// Edges whose inclusion in the independent set was blocked only by one node.
    pub blocked_by: Vec<Vec<EdgeIdx>>,
}

pub enum WithBlocked<I> {
    PruneBranch,
    ForcedNodes(I),
}

pub fn calculate(instance: &Instance, partial_size: usize) -> LowerBound {
    let mut edges: Vec<_> = instance.edges().iter().copied().collect();
    edges.sort_by_cached_key(|&edge_idx| {
        instance
            .edge(edge_idx)
            .fold((0, 0), |(sum, max), node_idx| {
                let degree = instance.node_degree(node_idx);
                (sum + degree, max.max(degree))
            })
    });

    let mut degrees = vec![0; instance.num_nodes_total()];
    for &node_idx in instance.nodes() {
        degrees[node_idx.idx()] = instance.node_degree(node_idx);
    }

    let mut hit = vec![false; instance.num_nodes_total()];
    let mut blocked_by = vec![vec![]; instance.num_nodes_total()];
    let mut edges_to_cover = instance.num_edges() as i64;
    let mut set_size = partial_size;
    for edge_idx in edges {
        let mut blocking_nodes_iter = instance
            .edge(edge_idx)
            .filter(|&node_idx| hit[node_idx.idx()]);

        if let Some(blocking_node) = blocking_nodes_iter.next() {
            if blocking_nodes_iter.next().is_none() {
                blocked_by[blocking_node.idx()].push(edge_idx);
            }
        } else {
            set_size += 1;
            let max_deg_node = instance
                .edge(edge_idx)
                .max_by_key(|&node_idx| instance.node_degree(node_idx))
                .expect("Empty edge in lower bound");
            edges_to_cover -= instance.node_degree(max_deg_node) as i64;

            for node_idx in instance.edge(edge_idx) {
                hit[node_idx.idx()] = true;
                degrees[node_idx.idx()] -= 1;
            }

            degrees[max_deg_node.idx()] = 0;
        }
    }

    degrees.sort_unstable();
    let degree_increase = degrees
        .into_iter()
        .rev()
        .take_while(|&degree| {
            if edges_to_cover <= 0 {
                return false;
            }
            edges_to_cover -= degree as i64;
            true
        })
        .count();

    LowerBound {
        lower_bound: set_size + degree_increase,
        independent_set_only_bound: set_size,
        blocked_by,
    }
}

pub fn calculate_with_blocked_nodes(
    instance: &Instance,
    partial_size: usize,
    smallest_known_size: usize,
) -> WithBlocked<impl Iterator<Item = NodeIdx> + '_> {
    let LowerBound {
        lower_bound,
        independent_set_only_bound,
        blocked_by,
    } = calculate(instance, partial_size);

    if lower_bound >= smallest_known_size {
        return WithBlocked::PruneBranch;
    }

    let mut undo_stack = vec![];
    let mut hit = vec![false; instance.num_nodes_total()];
    let forced_nodes_iter = instance.nodes().iter().copied().filter(move |&node_idx| {
        let mut new_lower_bound = independent_set_only_bound;
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

        new_lower_bound >= smallest_known_size
    });

    WithBlocked::ForcedNodes(forced_nodes_iter)
}
