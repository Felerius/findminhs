use crate::{
    create_idx_struct,
    data_structures::subset_trie::SubsetTrie,
    instance::{EdgeIdx, Instance, NodeIdx},
    report::Settings,
    small_indices::{IdxHashSet, SmallIdx},
};
use std::iter::Peekable;

create_idx_struct!(PackingIdx);

pub fn calc_max_degree_bound(instance: &Instance) -> Option<usize> {
    instance
        .nodes()
        .iter()
        .map(|&node| instance.node_degree(node))
        .max()
        .map(|max_degree| (instance.num_edges() + max_degree - 1) / max_degree)
}

pub fn calc_sum_degree_bound(instance: &Instance) -> usize {
    let mut degrees: Vec<_> = instance
        .nodes()
        .iter()
        .map(|&node| instance.node_degree(node))
        .collect();
    degrees.sort_unstable();

    let mut covered_edges = 0;
    degrees
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
        .count()
}

#[derive(Clone, Copy, Debug)]
pub struct EfficiencyBound(f64);

impl EfficiencyBound {
    pub fn round(self) -> Option<usize> {
        // If the calculated bound is less than EPSILON above an integer, it is
        // rounded down instead of up. This is used to avoid wrong bounds due to
        // floating point inaccuracies
        const EPSILON: f64 = 1e-9;

        let rounded = if self.0 < self.0.floor() + EPSILON {
            self.0.floor()
        } else {
            self.0.ceil()
        };

        if rounded.is_finite() {
            Some(rounded as usize)
        } else {
            None
        }
    }
}

pub fn calc_efficiency_bound(instance: &Instance) -> (EfficiencyBound, Vec<EfficiencyBound>) {
    let mut bound = EfficiencyBound(0.0);
    let mut discard_bounds = vec![EfficiencyBound(0.0); instance.num_nodes_total()];
    for &edge in instance.edges() {
        let (max_degree, max_degree_node, second_max_degree) =
            instance
                .edge(edge)
                .fold((0, NodeIdx::INVALID, 0), |(max, max_node, max2), node| {
                    let degree = instance.node_degree(node);
                    if degree > max {
                        (degree, node, max)
                    } else {
                        (max, max_node, max2.max(degree))
                    }
                });

        let bound_summand = (max_degree as f64).recip();
        bound.0 += bound_summand;
        if max_degree_node.valid() {
            let delta = (second_max_degree as f64).recip() - bound_summand;
            discard_bounds[max_degree_node.idx()].0 += delta;
        }
    }

    for discard_bound in &mut discard_bounds {
        discard_bound.0 += bound.0;
    }

    (bound, discard_bounds)
}

#[derive(Debug, Default)]
pub struct PackingBound {
    packing: Vec<EdgeIdx>,
}

impl PackingBound {
    pub fn new(instance: &Instance, settings: &Settings) -> Self {
        let mut packing: Vec<_> = instance.edges().to_vec();
        packing.sort_by_cached_key(|&edge| {
            instance.edge(edge).fold((0, 0), |(sum, max), node| {
                let degree = instance.node_degree(node);
                (sum + degree, max.max(degree))
            })
        });

        let mut disjoint = vec![true; instance.num_edges_total()];
        packing.retain(|&edge| {
            if !disjoint[edge.idx()] {
                return false;
            }

            for node in instance.edge(edge) {
                for overlapping_edge in instance.node(node) {
                    disjoint[overlapping_edge.idx()] = false;
                }
            }
            true
        });

        if settings.enable_local_search {
            packing = improve_packing_by_local_search(instance, packing);
        }

        Self { packing }
    }

    pub fn bound(&self) -> usize {
        self.packing.len()
    }

    pub fn calc_sum_over_packing_bound(&self, instance: &Instance) -> usize {
        let mut adjusted_degrees = vec![0; instance.num_nodes_total()];
        let mut covered_edges = 0;
        for &node in instance.nodes() {
            adjusted_degrees[node.idx()] = instance.node_degree(node);
        }

        for &packed_edge in &self.packing {
            let max_degree_node = instance
                .edge(packed_edge)
                .max_by_key(|&node| instance.node_degree(node))
                .expect("Empty edge in packing");
            covered_edges += instance.node_degree(max_degree_node);

            for node in instance.edge(packed_edge) {
                adjusted_degrees[node.idx()] -= 1;
            }

            adjusted_degrees[max_degree_node.idx()] = 0;
        }

        adjusted_degrees.sort_unstable();
        let sum_bound = adjusted_degrees
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

        self.packing.len() + sum_bound
    }

    pub fn calc_discard_bounds<'a>(
        &'a self,
        instance: &'a Instance,
    ) -> impl Iterator<Item = (NodeIdx, usize)> + 'a {
        let mut hit = vec![false; instance.num_nodes_total()];
        for &edge in &self.packing {
            for node in instance.edge(edge) {
                hit[node.idx()] = true;
            }
        }

        let mut blocked_by = vec![Vec::new(); instance.num_nodes_total()];
        let packing_set: IdxHashSet<_> = self.packing.iter().copied().collect();
        for &remaining_edge in instance.edges() {
            if packing_set.contains(&remaining_edge) {
                continue;
            }

            let mut blocking_nodes_iter = instance
                .edge(remaining_edge)
                .filter(|&node| hit[node.idx()]);
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
                let blocking_node = NodeIdx::from(idx);
                blocked.sort_by_cached_key(|&edge| {
                    instance.edge(edge).fold((0, 0), |(sum, max), node| {
                        let degree = instance.node_degree(node);
                        (sum + degree, max.max(degree))
                    })
                });

                blocked.retain(|&edge| {
                    let can_be_added = instance
                        .edge(edge)
                        .all(|node| node == blocking_node || !hit[node.idx()]);
                    if can_be_added {
                        for node in instance.edge(edge) {
                            hit[node.idx()] = true;
                        }
                        true
                    } else {
                        false
                    }
                });

                let result = if blocked.is_empty() {
                    None
                } else {
                    Some((blocking_node, self.packing.len() + blocked.len()))
                };

                for edge in blocked {
                    for node in instance.edge(edge) {
                        if node != blocking_node {
                            hit[node.idx()] = false;
                        }
                    }
                }

                result
            })
    }
}

#[derive(Clone)]
struct SetMinusIterator<T, I1, I2>(Peekable<I1>, Peekable<I2>)
where
    I1: Iterator<Item = T>,
    I2: Iterator<Item = T>;

impl<T, I1, I2> SetMinusIterator<T, I1, I2>
where
    I1: Iterator<Item = T>,
    I2: Iterator<Item = T>,
{
    fn new(
        set: impl IntoIterator<IntoIter = I1>,
        removed_set: impl IntoIterator<IntoIter = I2>,
    ) -> Self {
        Self(
            set.into_iter().peekable(),
            removed_set.into_iter().peekable(),
        )
    }
}

impl<T, I1, I2> Iterator for SetMinusIterator<T, I1, I2>
where
    I1: Iterator<Item = T>,
    I2: Iterator<Item = T>,
    T: Ord,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match (self.0.peek(), self.1.peek()) {
                (None, _) => return None,
                (Some(_), None) => return self.0.next(),
                (Some(item1), Some(item2)) if *item1 < *item2 => return self.0.next(),
                (Some(item1), Some(item2)) if *item1 == *item2 => {
                    self.0.next();
                    self.1.next();
                    continue;
                }
                (Some(_), Some(_)) => {
                    self.1.next();
                    continue;
                }
            }
        }
    }
}

fn find_two_opt_swap(
    instance: &Instance,
    available_nodes: &mut Vec<NodeIdx>,
    packing: &[EdgeIdx],
    blocked_by: &[Vec<EdgeIdx>],
    hit_by: &[PackingIdx],
) -> Option<(PackingIdx, (EdgeIdx, EdgeIdx))> {
    available_nodes.clear();
    available_nodes.extend(
        instance
            .nodes()
            .iter()
            .copied()
            .filter(|node| !hit_by[node.idx()].valid()),
    );

    for (blocking, blocked) in blocked_by.iter().enumerate() {
        if blocked.is_empty() {
            continue;
        }

        let blocking_edge = packing[blocking];
        available_nodes.extend(instance.edge(blocking_edge));
        available_nodes.sort_unstable();
        let mut trie: SubsetTrie<_, EdgeIdx, _> = SubsetTrie::new(instance.num_nodes_total());

        for &blocked_edge in blocked {
            let available_iter =
                SetMinusIterator::new(available_nodes.iter().copied(), instance.edge(blocked_edge));
            let other_edge = trie.find_subset(available_iter);

            if other_edge.valid() {
                return Some((PackingIdx::from(blocking), (blocked_edge, other_edge)));
            }

            trie.insert(blocked_edge, instance.edge(blocked_edge));
        }

        available_nodes.retain(|node| !hit_by[node.idx()].valid());
    }

    None
}

fn improve_packing_by_local_search(instance: &Instance, mut packing: Vec<EdgeIdx>) -> Vec<EdgeIdx> {
    let packing_set: IdxHashSet<_> = packing.iter().copied().collect();
    let mut remaining: Vec<_> = instance
        .edges()
        .iter()
        .copied()
        .filter(|edge| !packing_set.contains(edge))
        .collect();

    // Reuse some allocations across local search iterations
    let mut hit_by = vec![PackingIdx::INVALID; instance.num_nodes_total()];
    let mut blocked_by: Vec<Vec<_>> = Vec::new();
    let mut available_nodes = Vec::new();

    loop {
        // For each node, calculate which packing edge is hitting it (if any)
        hit_by.fill(PackingIdx::INVALID);
        for (idx, &packing_edge) in packing.iter().enumerate() {
            for node in instance.edge(packing_edge) {
                hit_by[node.idx()] = PackingIdx::from(idx);
            }
        }

        // Group remaining edges only blocked by a single packing edge by the blocking packing edge
        for blocked_by_list in &mut blocked_by {
            blocked_by_list.clear();
        }
        blocked_by.resize(packing.len(), Vec::new());
        for &remaining_edge in &remaining {
            let mut blocking = PackingIdx::INVALID;
            for node in instance.edge(remaining_edge) {
                if !hit_by[node.idx()].valid() {
                    continue;
                }

                if blocking.valid() && blocking != hit_by[node.idx()] {
                    // Found second edge blocking this ones inclusion
                    blocking = PackingIdx::INVALID;
                    break;
                }
                blocking = hit_by[node.idx()];
            }

            // We assume that each remaining edge is blocked by at least one edge, thus could not
            // simply be added to the packing. Thus, blocking is invalid if and only if this edge
            // was blocked by multiple packing edges.
            if blocking.valid() {
                blocked_by[blocking.idx()].push(remaining_edge);
            }
        }

        let two_opt_swap = find_two_opt_swap(
            instance,
            &mut available_nodes,
            &packing,
            &blocked_by,
            &hit_by,
        );
        let (removed_edge_idx, (added_edge1, added_edge2)) = match two_opt_swap {
            Some(tuple) => tuple,
            None => return packing,
        };

        let removed_edge = packing[removed_edge_idx.idx()];
        packing.retain(|&edge| edge != removed_edge);
        remaining.retain(|&edge| edge != added_edge1 && edge != added_edge2);
        packing.push(added_edge1);
        packing.push(added_edge2);
        remaining.push(removed_edge);

        // Due to the swap, other edges previously blocked by removed_edge might now be addable to
        // the packing. Since we generally assume that no edge can just be added to the packing, we
        // find and add them here.
        for node in instance.edge(removed_edge) {
            hit_by[node.idx()] = PackingIdx::INVALID;
        }

        // Dummy packing idx used to mark hit nodes (since we only care whether nodes are hit here,
        // not by whom)
        let dummy_idx = PackingIdx(0);
        for node in instance.edge(added_edge1).chain(instance.edge(added_edge2)) {
            hit_by[node.idx()] = dummy_idx;
        }

        for &packing_candidate_edge in &blocked_by[removed_edge_idx.idx()] {
            if instance
                .edge(packing_candidate_edge)
                .all(|node| !hit_by[node.idx()].valid())
            {
                packing.push(packing_candidate_edge);
                remaining.retain(|&edge| edge != packing_candidate_edge);
                for node in instance.edge(packing_candidate_edge) {
                    hit_by[node.idx()] = dummy_idx;
                }
            }
        }
    }
}
