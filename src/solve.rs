#[cfg(feature = "branching-activity")]
use crate::activity::Activities;
use crate::{
    instance::{Instance, NodeIdx},
    reductions::{self, Reduction},
    small_indices::{IdxHashSet, SmallIdx},
};
use anyhow::Result;
use cfg_if::cfg_if;
use log::{debug, info, trace, warn};
#[cfg(feature = "branching-random")]
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use std::time::{Duration, Instant};

const ITER_LOG_GAP: u64 = 60;

#[derive(Debug, Clone)]
pub struct Stats {
    pub iterations: usize,
    pub reduction_time: Duration,
    pub last_iter_log: Instant,
}

#[derive(Debug, Clone)]
struct State<R: Rng> {
    rng: R,

    /// All nodes in the partial HS, including those added by reductions
    partial_hs: Vec<NodeIdx>,

    /// Smallest known HS
    smallest_known: Vec<NodeIdx>,

    #[cfg(feature = "branching-activity")]
    activities: Activities,

    stats: Stats,

    instance_name: String,
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

fn expensive_lower_bound(instance: &Instance, partial_size: usize) -> usize {
    let mut edges: Vec<_> = instance.edges().iter().copied().collect();
    edges.sort_by_cached_key(|&edge_idx| {
        instance
            .edge(edge_idx)
            .map(|node_idx| instance.node_degree(node_idx))
            .sum::<usize>()
    });

    let mut hit = vec![false; instance.num_nodes_total()];
    let mut lower_bound = partial_size;
    for edge_idx in edges {
        if instance.edge(edge_idx).all(|node_idx| !hit[node_idx.idx()]) {
            lower_bound += 1;
            for node_idx in instance.edge(edge_idx) {
                hit[node_idx.idx()] = true;
            }
        }
    }

    lower_bound
}

fn cheap_lower_bound(instance: &Instance, partial_size: usize) -> usize {
    let max_node_degree = instance.max_node_degree().0;
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

    #[cfg(feature = "branching-activity")]
    state.activities.delete(node_idx);

    // Randomize branching order
    let take_first: bool = state.rng.gen();
    for &take in &[take_first, !take_first] {
        if take {
            instance.delete_incident_edges(node_idx);
            state.partial_hs.push(node_idx);
        }

        solve_recursive(instance, state);

        if take {
            debug_assert_eq!(state.partial_hs.last().copied(), Some(node_idx));
            state.partial_hs.pop();
            instance.restore_incident_edges(node_idx);
        }
    }

    #[cfg(feature = "branching-activity")]
    state.activities.restore(node_idx);
    instance.restore_node(node_idx);
}

#[allow(unused_variables)]
fn pick_branching_node(instance: &Instance, state: &mut State<impl Rng>) -> NodeIdx {
    cfg_if! {
        if #[cfg(feature = "branching-activity")] {
            state.activities.highest()
        } else if #[cfg(feature = "branching-random")] {
            *instance.nodes().choose(&mut state.rng).expect("check for no nodes failed")
        } else if #[cfg(feature = "branching-degree")] {
            instance.max_node_degree().1
        } else {
            compile_error!("no branching-* feature selected")
        }
    }
}

fn solve_recursive(instance: &mut Instance, state: &mut State<impl Rng>) {
    state.stats.iterations += 1;
    let smallest_known_size = state.smallest_known.len();

    let now = Instant::now();
    if (now - state.stats.last_iter_log).as_secs() >= ITER_LOG_GAP {
        info!(
            "Running on {} for {} iterations",
            &state.instance_name, state.stats.iterations
        );
        state.stats.last_iter_log = now;
    }

    // Don't run reductions on the first iteration, we already do so before
    // calculating the greedy approximation
    let reduction = if state.stats.iterations > 1 {
        reductions::reduce(
            instance,
            &mut state.partial_hs,
            &mut state.stats,
            |instance, partial_hs| match instance.min_edge_degree() {
                None | Some((0, _)) => true,
                _ => cheap_lower_bound(instance, partial_hs.len()) >= smallest_known_size,
            },
        )
    } else {
        Reduction::default()
    };

    #[cfg(feature = "branching-activity")]
    for removed_node_idx in reduction.nodes() {
        state.activities.delete(removed_node_idx)
    }

    if expensive_lower_bound(instance, state.partial_hs.len()) >= smallest_known_size {
        // Instance unsolvable or lower bound exceeds best known size
        #[cfg(feature = "branching-activity")]
        {
            for &node in &state.partial_hs {
                state.activities.bump(node);
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
        let node = pick_branching_node(instance, state);
        branch_on(node, instance, state);
    }

    reduction.restore(instance, &mut state.partial_hs);

    #[cfg(feature = "branching-activity")]
    for node in reduction.nodes() {
        state.activities.restore(node);
    }
}

pub fn solve(
    mut instance: Instance,
    rng: impl Rng + SeedableRng,
    instance_name: String,
) -> Result<SolveResult> {
    let mut state = State {
        rng,
        partial_hs: Vec::new(),
        smallest_known: Vec::new(),
        #[cfg(feature = "branching-activity")]
        activities: Activities::new(instance.num_nodes_total()),
        stats: Stats {
            iterations: 0,
            reduction_time: Duration::default(),
            last_iter_log: Instant::now(),
        },
        instance_name,
    };

    let initial_reduction = reductions::reduce(
        &mut instance,
        &mut state.partial_hs,
        &mut state.stats,
        |_, _| false,
    );
    info!("Initial reduction time: {:.2?}", state.stats.reduction_time);

    #[cfg(feature = "branching-activity")]
    for node_idx in initial_reduction.nodes() {
        state.activities.delete(node_idx);
    }

    let time_start = Instant::now();
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

    info!("Validating found hitting set");
    initial_reduction.restore(&mut instance, &mut state.partial_hs);
    let hs_set: IdxHashSet<_> = state.smallest_known.iter().copied().collect();
    assert_eq!(instance.num_nodes_total(), instance.nodes().len());
    assert_eq!(instance.num_edges_total(), instance.edges().len());
    for &edge_idx in instance.edges() {
        let hit = instance
            .edge(edge_idx)
            .any(|node_idx| hs_set.contains(&node_idx));
        assert!(hit, "edge {} not hit", edge_idx);
    }

    Ok(SolveResult {
        hs_size: state.smallest_known.len(),
        greedy_size,
        solve_time: solve_time.as_secs_f64(),
        stats: state.stats,
    })
}
