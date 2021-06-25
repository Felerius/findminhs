use crate::{
    data_structures::{subset_trie::SubsetTrie, superset_trie::SupersetTrie},
    instance::{EdgeIdx, Instance, NodeIdx},
    lower_bound::{self, EfficiencyBound, PackingBound},
    report::{GreedyMode, ReductionStats, RuntimeStats, Settings},
    small_indices::{IdxHashSet, SmallIdx},
};
use log::info;
use std::{
    cmp::Reverse,
    collections::BinaryHeap,
    time::{Duration, Instant},
};

#[derive(Copy, Clone, Debug)]
enum ReducedItem {
    RemovedNode(NodeIdx),
    RemovedEdge(EdgeIdx),
    ForcedNode(NodeIdx),
}

impl ReducedItem {
    fn apply(self, instance: &mut Instance, partial_hs: &mut Vec<NodeIdx>) {
        match self {
            Self::RemovedNode(node) => instance.delete_node(node),
            Self::RemovedEdge(edge) => instance.delete_edge(edge),
            Self::ForcedNode(node) => {
                instance.delete_node(node);
                instance.delete_incident_edges(node);
                partial_hs.push(node);
            }
        }
    }

    fn restore(self, instance: &mut Instance, partial_hs: &mut Vec<NodeIdx>) {
        match self {
            Self::RemovedNode(node) => instance.restore_node(node),
            Self::RemovedEdge(edge) => instance.restore_edge(edge),
            Self::ForcedNode(node) => {
                instance.restore_incident_edges(node);
                instance.restore_node(node);
                debug_assert_eq!(partial_hs.last().copied(), Some(node));
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
    nodes.into_iter().filter_map(move |node| {
        if trie.contains_superset(instance.node(node)) {
            Some(ReducedItem::RemovedNode(node))
        } else {
            trie.insert(instance.node(node));
            None
        }
    })
}

fn find_dominated_edges(instance: &Instance) -> impl Iterator<Item = ReducedItem> + '_ {
    let mut edges = instance.edges().to_vec();
    edges.sort_unstable_by_key(|&edge| instance.edge_size(edge));
    let mut trie = SubsetTrie::new(instance.num_nodes_total());
    edges.into_iter().filter_map(move |edge| {
        if trie.find_subset(instance.edge(edge)) {
            Some(ReducedItem::RemovedEdge(edge))
        } else {
            trie.insert(true, instance.edge(edge));
            None
        }
    })
}

fn find_forced_nodes(instance: &Instance) -> impl Iterator<Item = ReducedItem> {
    let forced: IdxHashSet<_> = instance
        .edges()
        .iter()
        .copied()
        .filter_map(|edge| {
            let mut edge_nodes_iter = instance.edge(edge);
            edge_nodes_iter.next().and_then(|first_node| {
                if edge_nodes_iter.next().is_some() {
                    None
                } else {
                    Some(first_node)
                }
            })
        })
        .collect();
    forced.into_iter().map(ReducedItem::ForcedNode)
}

fn find_costly_discards_using_efficiency_bound<'a>(
    instance: &'a Instance,
    lower_bound_breakpoint: usize,
    discard_efficieny_bounds: &'a [EfficiencyBound],
) -> impl Iterator<Item = ReducedItem> + 'a {
    instance
        .nodes()
        .iter()
        .copied()
        .filter(move |node| {
            discard_efficieny_bounds[node.idx()]
                .round()
                .unwrap_or(usize::MAX)
                >= lower_bound_breakpoint
        })
        .map(ReducedItem::ForcedNode)
}

fn find_costly_discards_using_packing_update<'a>(
    instance: &'a Instance,
    lower_bound_breakpoint: usize,
    packing_bound: &'a PackingBound,
) -> impl Iterator<Item = ReducedItem> + 'a {
    packing_bound
        .calc_discard_bounds(instance)
        .filter_map(move |(node, new_bound)| {
            if new_bound >= lower_bound_breakpoint {
                Some(ReducedItem::ForcedNode(node))
            } else {
                None
            }
        })
}

fn find_costly_discard_using_packing_from_scratch(
    instance: &mut Instance,
    lower_bound_breakpoint: usize,
    settings: &Settings,
) -> Option<(ReducedItem, usize)> {
    if settings.packing_from_scratch_limit == 0 {
        return None;
    }

    let mut nodes = instance.nodes().to_vec();
    nodes.sort_unstable_by_key(|&node| Reverse(instance.node_degree(node)));
    nodes
        .into_iter()
        .take(settings.packing_from_scratch_limit)
        .enumerate()
        .find_map(|(idx, node)| {
            instance.delete_node(node);
            let packing_bound = PackingBound::new(instance, settings);
            let new_lower_bound = packing_bound.calc_sum_over_packing_bound(instance);
            instance.restore_node(node);

            if new_lower_bound >= lower_bound_breakpoint {
                Some((ReducedItem::ForcedNode(node), idx))
            } else {
                None
            }
        })
}

pub fn calc_greedy_approximation(instance: &Instance) -> Vec<NodeIdx> {
    let mut hit = vec![true; instance.num_edges_total()];
    for edge in instance.edges() {
        hit[edge.idx()] = false;
    }
    let mut node_degrees = vec![0; instance.num_nodes_total()];
    let mut node_queue = BinaryHeap::new();
    for &node in instance.nodes() {
        node_degrees[node.idx()] = instance.node_degree(node);
        node_queue.push((node_degrees[node.idx()], node));
    }

    let mut hs = Vec::new();
    while let Some((degree, node)) = node_queue.pop() {
        if degree == 0 {
            break;
        }
        if degree > node_degrees[node.idx()] {
            continue;
        }

        hs.push(node);
        node_degrees[node.idx()] = 0; // Fewer elements in the heap
        for edge in instance.node(node) {
            if hit[edge.idx()] {
                continue;
            }

            hit[edge.idx()] = true;
            for edge_node in instance.edge(edge) {
                if node_degrees[edge_node.idx()] > 0 {
                    node_degrees[edge_node.idx()] -= 1;
                    node_queue.push((node_degrees[edge_node.idx()], edge_node));
                }
            }
        }
    }

    hs
}

fn recalculate_greedy_upper_bound(
    instance: &Instance,
    partial_hs: &[NodeIdx],
    minimum_hs: &mut Vec<NodeIdx>,
    runtimes: &mut RuntimeStats,
    stats: &mut ReductionStats,
) {
    stats.greedy_runs += 1;
    collect_time_info(&mut runtimes.greedy, || {
        let greedy = calc_greedy_approximation(instance);
        if partial_hs.len() + greedy.len() < minimum_hs.len() {
            stats.greedy_bound_improvements += 1;
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
    });
}

fn collect_time_info<T>(runtime: &mut Duration, func: impl FnOnce() -> T) -> T {
    let before = Instant::now();
    let result = func();
    *runtime += before.elapsed();
    result
}

fn run_reduction<I>(
    reduced_items: &mut Vec<ReducedItem>,
    runtime: &mut Duration,
    runs: &mut usize,
    item_counter: &mut usize,
    func: impl FnOnce() -> I,
) where
    I: IntoIterator<Item = ReducedItem>,
{
    let len_before = reduced_items.len();
    *runs += 1;
    collect_time_info(runtime, || {
        reduced_items.extend(func());
    });
    *item_counter += reduced_items.len() - len_before;
}

pub fn reduce(
    instance: &mut Instance,
    partial_hs: &mut Vec<NodeIdx>,
    minimum_hs: &mut Vec<NodeIdx>,
    runtimes: &mut RuntimeStats,
    stats: &mut ReductionStats,
    settings: &Settings,
) -> (ReductionResult, Reduction) {
    let mut reduced_items = Vec::new();

    if settings.greedy_mode == GreedyMode::Once {
        recalculate_greedy_upper_bound(instance, &partial_hs, minimum_hs, runtimes, stats);
    }

    let result = loop {
        if partial_hs.len() >= minimum_hs.len() {
            break ReductionResult::Unsolvable;
        }

        if instance.num_edges() == 0 {
            break ReductionResult::Solved;
        }

        if settings.greedy_mode == GreedyMode::AlwaysBeforeBounds {
            recalculate_greedy_upper_bound(instance, &partial_hs, minimum_hs, runtimes, stats);
            if partial_hs.len() >= minimum_hs.len() {
                break ReductionResult::Unsolvable;
            }
        }

        let mut lower_bound_breakpoint = minimum_hs.len() - partial_hs.len();
        if settings.enable_max_degree_bound {
            let max_degree_bound = collect_time_info(&mut runtimes.max_degree_bound, || {
                lower_bound::calc_max_degree_bound(instance).unwrap_or(usize::MAX)
            });
            if max_degree_bound >= lower_bound_breakpoint {
                stats.max_degree_bound_breaks += 1;
                break ReductionResult::Unsolvable;
            }
        }

        if settings.enable_sum_degree_bound {
            let sum_degree_bound = collect_time_info(&mut runtimes.sum_degree_bound, || {
                lower_bound::calc_sum_degree_bound(instance)
            });
            if sum_degree_bound >= lower_bound_breakpoint {
                stats.sum_degree_bound_breaks += 1;
                break ReductionResult::Unsolvable;
            }
        }

        let discard_efficiency_bounds = if settings.enable_efficiency_bound {
            let (efficiency_bound, discard_efficiency_bounds) =
                collect_time_info(&mut runtimes.efficiency_bound, || {
                    lower_bound::calc_efficiency_bound(instance)
                });
            if efficiency_bound.round().unwrap_or(usize::MAX) >= lower_bound_breakpoint {
                stats.efficiency_degree_bound_breaks += 1;
                break ReductionResult::Unsolvable;
            }
            discard_efficiency_bounds
        } else {
            Vec::new()
        };

        let packing_bound = if settings.enable_packing_bound {
            let packing_bound = collect_time_info(&mut runtimes.packing_bound, || {
                PackingBound::new(instance, settings)
            });
            if packing_bound.bound() >= lower_bound_breakpoint {
                stats.packing_bound_breaks += 1;
                break ReductionResult::Unsolvable;
            }
            packing_bound
        } else {
            PackingBound::default()
        };

        if settings.enable_packing_bound && settings.enable_sum_over_packing_bound {
            let sum_over_packing_bound =
                collect_time_info(&mut runtimes.sum_over_packing_bound, || {
                    packing_bound.calc_sum_over_packing_bound(instance)
                });
            if sum_over_packing_bound >= lower_bound_breakpoint {
                stats.sum_over_packing_bound_breaks += 1;
                break ReductionResult::Unsolvable;
            }
        }

        let unchanged_len = reduced_items.len();
        run_reduction(
            &mut reduced_items,
            &mut runtimes.forced_vertex,
            &mut stats.forced_vertex_runs,
            &mut stats.forced_vertices_found,
            || find_forced_nodes(instance),
        );

        if reduced_items.len() == unchanged_len {
            // Do not time this step as all costly parts are integrated into the
            // calculation of the efficiency bound above. This steps just checks
            // the already calculated discard bounds against the breakpoint
            let mut dummy_duration = Duration::default();
            run_reduction(
                &mut reduced_items,
                &mut dummy_duration,
                &mut stats.costly_discard_efficiency_runs,
                &mut stats.costly_discard_efficiency_vertices_found,
                || {
                    find_costly_discards_using_efficiency_bound(
                        instance,
                        lower_bound_breakpoint,
                        &discard_efficiency_bounds,
                    )
                },
            );
        }

        if reduced_items.len() == unchanged_len {
            run_reduction(
                &mut reduced_items,
                &mut runtimes.costly_discard_packing_update,
                &mut stats.costly_discard_packing_update_runs,
                &mut stats.costly_discard_packing_update_vertices_found,
                || {
                    find_costly_discards_using_packing_update(
                        instance,
                        lower_bound_breakpoint,
                        &packing_bound,
                    )
                },
            );
        }

        if reduced_items.len() == unchanged_len
            && settings.greedy_mode == GreedyMode::AlwaysBeforeExpensiveReductions
        {
            recalculate_greedy_upper_bound(instance, &partial_hs, minimum_hs, runtimes, stats);
            if partial_hs.len() >= minimum_hs.len() {
                break ReductionResult::Unsolvable;
            }
            lower_bound_breakpoint = minimum_hs.len() - partial_hs.len();
        }

        if reduced_items.len() == unchanged_len {
            let table_ref = &mut stats.costly_discard_packing_from_scratch_steps_per_run;
            let mut dummy_counter = 0;
            run_reduction(
                &mut reduced_items,
                &mut runtimes.costly_discard_packing_from_scratch,
                &mut stats.costly_discard_packing_from_scratch_runs,
                &mut dummy_counter,
                || {
                    let result = find_costly_discard_using_packing_from_scratch(
                        instance,
                        lower_bound_breakpoint,
                        settings,
                    );
                    match result {
                        None => {
                            table_ref[settings.packing_from_scratch_limit] += 1;
                            None
                        }
                        Some((item, idx)) => {
                            table_ref[idx] += 1;
                            Some(item)
                        }
                    }
                },
            );
        }

        if reduced_items.len() == unchanged_len {
            run_reduction(
                &mut reduced_items,
                &mut runtimes.vertex_domination,
                &mut stats.vertex_dominations_runs,
                &mut stats.vertex_dominations_vertices_found,
                || find_dominated_nodes(instance),
            );
        }

        if reduced_items.len() == unchanged_len {
            run_reduction(
                &mut reduced_items,
                &mut runtimes.edge_domination,
                &mut stats.edge_dominations_runs,
                &mut stats.edge_dominations_edges_found,
                || find_dominated_edges(instance),
            );
        }

        if reduced_items.len() == unchanged_len {
            break ReductionResult::Finished;
        }

        collect_time_info(&mut runtimes.applying_reductions, || {
            for reduced_item in &reduced_items[unchanged_len..] {
                reduced_item.apply(instance, partial_hs);
            }
        });
    };

    (result, Reduction(reduced_items))
}
