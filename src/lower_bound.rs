use crate::{
    instance::{EdgeIdx, Instance},
    small_indices::SmallIdx,
};

pub fn pack_edges(instance: &Instance) -> (Vec<EdgeIdx>, Vec<EdgeIdx>) {
    let mut edges: Vec<_> = instance.edges().iter().copied().collect();
    edges.sort_by_cached_key(|&edge_idx| {
        instance
            .edge(edge_idx)
            .fold((0, 0), |(sum, max), node_idx| {
                let degree = instance.node_degree(node_idx);
                (sum + degree, max.max(degree))
            })
    });

    let mut hit = vec![false; instance.num_nodes_total()];
    let mut packing = Vec::new();
    edges.retain(|&edge_idx| {
        if instance.edge(edge_idx).all(|node_idx| !hit[node_idx.idx()]) {
            packing.push(edge_idx);
            for node_idx in instance.edge(edge_idx) {
                hit[node_idx.idx()] = true;
            }

            false
        } else {
            true
        }
    });

    (packing, edges)
}

pub fn calculate(instance: &Instance, packing: &[EdgeIdx], partial_size: usize) -> usize {
    let mut degree = vec![0; instance.num_nodes_total()];
    let mut covered_edges = 0;
    for &node_idx in instance.nodes() {
        degree[node_idx.idx()] = instance.node_degree(node_idx);
    }

    for &packed_edge in packing {
        let max_degree_node = instance
            .edge(packed_edge)
            .max_by_key(|&node_idx| instance.node_degree(node_idx))
            .expect("Empty edge in packing");
        covered_edges += instance.node_degree(max_degree_node);

        for node_idx in instance.edge(packed_edge) {
            degree[node_idx.idx()] -= 1;
        }

        degree[max_degree_node.idx()] = 0;
    }

    degree.sort_unstable();
    let sum_bound = degree
        .into_iter()
        .rev()
        .take_while(|&degree| {
            if covered_edges < instance.num_edges() {
                covered_edges += degree;
                true
            } else {
                false
            }
        })
        .count();

    partial_size + packing.len() + sum_bound
}
