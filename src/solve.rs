#[cfg(not(feature = "activity-disable"))]
use crate::activity::Activities;
use crate::instance::{Instance, NodeIdx};
use crate::reductions::{self, Reduction};
use crate::small_indices::SmallIdx;
use anyhow::Result;
use log::{debug, info, trace, warn};
#[cfg(feature = "activity-disable")]
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub iterations: usize,
    pub reduction_time: Duration,
}

#[derive(Debug, Clone)]
struct State<R: Rng> {
    rng: R,

    /// All nodes in the partial HS, including those added by reductions
    partial_hs: Vec<NodeIdx>,

    /// All nodes added to the partial HS as a branching decision
    taken: Vec<NodeIdx>,

    /// All nodes discarded as a branching decision
    discarded: Vec<NodeIdx>,

    /// Smallest known HS
    smallest_known: Vec<NodeIdx>,

    #[cfg(not(feature = "activity-disable"))]
    activities: Activities,

    stats: Stats,
}

#[derive(Debug, Clone)]
#[allow(clippy::module_name_repetitions)]
pub struct SolveResult {
    pub hs_size: usize,
    pub greedy_size: usize,
    pub solve_time: f64,
    pub stats: Stats,
}

fn greedy_approx(instance: &mut Instance, base_hs: Vec<NodeIdx>) -> Vec<NodeIdx> {
    let time_start = Instant::now();
    let initial_size = base_hs.len();
    let mut hs = base_hs;
    while !instance.edges().is_empty() {
        let mut max_degree = (0, NodeIdx::INVALID);
        for &node in instance.nodes() {
            max_degree = max_degree.max((instance.node_degree(node), node));
        }
        instance.delete_node(max_degree.1);
        instance.delete_incident_edges(max_degree.1);
        hs.push(max_degree.1);
    }
    for &node in hs[initial_size..].iter().rev() {
        instance.restore_incident_edges(node);
        instance.restore_node(node);
    }
    info!(
        "Greedy hs of size {} ({:.2?})",
        hs.len(),
        Instant::now() - time_start
    );
    hs
}

fn lower_bound(instance: &Instance, partial_size: usize) -> usize {
    let max_node_degree = instance.max_node_degree();
    let num_edges = instance.num_edges();
    if max_node_degree == 0 {
        // Instance already solved
        return partial_size;
    }
    let rem_lower_bound = (num_edges + max_node_degree - 1) / max_node_degree;
    partial_size + rem_lower_bound
}

fn branch_on(node_idx: NodeIdx, instance: &mut Instance, state: &mut State<impl Rng>) {
    trace!("Branching on {}", node_idx);

    instance.delete_node(node_idx);

    #[cfg(not(feature = "activity-disable"))]
    state.activities.delete(node_idx);

    // Randomize branching order
    let take_first: bool = state.rng.gen();
    for &take in &[take_first, !take_first] {
        if take {
            instance.delete_incident_edges(node_idx);
            state.partial_hs.push(node_idx);
            state.taken.push(node_idx);

            solve_recursive(instance, state);

            debug_assert_eq!(state.taken.last().copied(), Some(node_idx));
            state.taken.pop();
            debug_assert_eq!(state.partial_hs.last().copied(), Some(node_idx));
            state.partial_hs.pop();
            instance.restore_incident_edges(node_idx);
        } else {
            state.discarded.push(node_idx);
            solve_recursive(instance, state);
            debug_assert_eq!(state.discarded.last().copied(), Some(node_idx));
            state.discarded.pop();
        }
    }

    #[cfg(not(feature = "activity-disable"))]
    state.activities.restore(node_idx);
    instance.restore_node(node_idx);
}

fn solve_recursive(instance: &mut Instance, state: &mut State<impl Rng>) {
    state.stats.iterations += 1;
    let smallest_known_size = state.smallest_known.len();

    // Don't run reductions on the first iteration, we already do so before
    // calculating the greedy approximation
    let reduction = if state.stats.iterations > 1 {
        reductions::reduce(
            instance,
            &mut state.partial_hs,
            &mut state.stats,
            |instance, partial_hs| match instance.min_edge_degree().map(|(deg, _idx)| deg) {
                None | Some(0) => true,
                _ => lower_bound(instance, partial_hs.len()) >= smallest_known_size,
            },
        )
    } else {
        Reduction::default()
    };

    #[cfg(not(feature = "activity-disable"))]
    for removed_node_idx in reduction.nodes() {
        state.activities.delete(removed_node_idx)
    }

    if instance.edges().is_empty() {
        // Instance is solved
        if state.partial_hs.len() < smallest_known_size {
            let bound = lower_bound(instance, state.partial_hs.len());
            if bound >= smallest_known_size {
                log::error!(
                    "Have smaller hs ({} vs. {}), but lower bound signals abort ({} vs. {})",
                    state.partial_hs.len(),
                    smallest_known_size,
                    bound,
                    smallest_known_size
                );
            }
        }
    }

    if lower_bound(instance, state.partial_hs.len()) >= smallest_known_size {
        // Instance unsolvable or lower bound exceeds best known size
        #[cfg(not(feature = "activity-disable"))]
        {
            #[allow(clippy::cast_precision_loss)]
            let bump_amount = if cfg!(feature = "activity-relative") {
                let depth = state.partial_hs.len() + state.discarded.len();
                1.0 / depth as f64
            } else {
                1.0
            };

            for &node in &state.partial_hs {
                state.activities.bump(node, (bump_amount, 0.0));
            }

            for &node in &state.discarded {
                state.activities.bump(node, (0.0, bump_amount));
            }

            state.activities.decay();
        }
    } else if instance.edges().is_empty() {
        // Instance is solved
        if state.partial_hs.len() < smallest_known_size {
            info!("Found HS of size {}", state.partial_hs.len());
            state.smallest_known.truncate(state.partial_hs.len());
            state.smallest_known.copy_from_slice(&state.partial_hs);
        } else {
            warn!(
                "Found HS is not smaller than best known ({} vs. {}), should have been pruned",
                state.partial_hs.len(),
                state.smallest_known.len()
            );
        }
    } else {
        #[cfg(feature = "activity-disable")]
        let node = *instance
            .nodes()
            .choose(&mut state.rng)
            .expect("Check for no nodes failed");
        #[cfg(not(feature = "activity-disable"))]
        let node = state.activities.highest();
        branch_on(node, instance, state);
    }

    reduction.restore(instance, &mut state.partial_hs);

    #[cfg(not(feature = "activity-disable"))]
    for node in reduction.nodes() {
        state.activities.restore(node);
    }
}

pub fn solve(mut instance: Instance, rng: impl Rng + SeedableRng) -> Result<SolveResult> {
    let time_start = Instant::now();
    let mut state = State {
        rng,
        partial_hs: Vec::new(),
        taken: Vec::new(),
        discarded: Vec::new(),
        smallest_known: Vec::new(),
        #[cfg(not(feature = "activity-disable"))]
        activities: Activities::new(instance.num_nodes_total()),
        stats: Stats::default(),
    };

    let initial_reduction = reductions::reduce(
        &mut instance,
        &mut state.partial_hs,
        &mut state.stats,
        |_, _| false,
    );
    info!("Initial reduction time: {:.2?}", state.stats.reduction_time);

    #[cfg(not(feature = "activity-disable"))]
    for node_idx in initial_reduction.nodes() {
        state.activities.delete(node_idx);
    }

    state.smallest_known = greedy_approx(&mut instance, state.partial_hs.clone());
    let greedy_size = state.smallest_known.len();

    solve_recursive(&mut instance, &mut state);
    let solve_time = Instant::now() - time_start;

    info!(
        "Solving took {} iterations ({:.2?})",
        state.stats.iterations, solve_time
    );
    debug!(
        "Final HS (size {}): {:?}",
        state.smallest_known.len(),
        &state.smallest_known
    );

    Ok(SolveResult {
        hs_size: state.smallest_known.len(),
        greedy_size,
        solve_time: solve_time.as_secs_f64(),
        stats: state.stats,
    })
}
