use crate::{
    instance::{Instance, NodeIdx},
    reductions::{self, Reduction},
    small_indices::{IdxHashSet, SmallIdx},
};
use anyhow::Result;
use log::{debug, info, trace, warn};
use rand::{Rng, SeedableRng};
use serde::{Serialize, Serializer};
use std::time::{Duration, Instant};

const ITER_LOG_GAP: u64 = 60;

fn serialize_duration_as_seconds<S>(duration: &Duration, ser: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    ser.serialize_f64(duration.as_secs_f64())
}

#[allow(clippy::clippy::ptr_arg)]
fn serialize_hitting_set_as_size<S>(hs: &Vec<NodeIdx>, ser: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    ser.serialize_u64(hs.len() as u64)
}

#[derive(Debug, Clone, Serialize)]
pub struct Solution {
    /// Name of the input file for the instance
    pub file_name: String,

    /// Minimum hitting set
    #[serde(rename = "hs_size", serialize_with = "serialize_hitting_set_as_size")]
    pub minimum_hs: Vec<NodeIdx>,

    /// Size of the greedily found initial hitting set
    pub greedy_size: usize,

    /// How often the branching algorithm has branched
    pub branching_steps: usize,

    /// Total time required to solve the instance
    #[serde(serialize_with = "serialize_duration_as_seconds")]
    pub runtime: Duration,

    /// How long the initial reduction took
    #[serde(serialize_with = "serialize_duration_as_seconds")]
    pub initial_reduction_runtime: Duration,

    /// Seed for the random number generator
    pub seed: u64,
}

#[derive(Debug, Clone)]
struct State<R: Rng> {
    rng: R,
    partial_hs: Vec<NodeIdx>,
    solution: Solution,
    last_log_time: Instant,
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
    state.solution.branching_steps += 1;

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

    instance.restore_node(node_idx);
}

fn solve_recursive(instance: &mut Instance, state: &mut State<impl Rng>) {
    let smallest_known_size = state.solution.minimum_hs.len();

    let now = Instant::now();
    if (now - state.last_log_time).as_secs() >= ITER_LOG_GAP {
        info!(
            "Running on {} for {} branching steps",
            &state.solution.file_name, state.solution.branching_steps
        );
        state.last_log_time = now;
    }

    // Don't run reductions on the first iteration, we already do so before
    // calculating the greedy approximation
    let reduction =
        if state.solution.branching_steps == 0 {
            Reduction::default()
        } else {
            reductions::reduce(instance, &mut state.partial_hs, |instance, partial_hs| {
                match instance.min_edge_degree() {
                    None | Some((0, _)) => true,
                    _ => cheap_lower_bound(instance, partial_hs.len()) >= smallest_known_size,
                }
            })
        };

    if expensive_lower_bound(instance, state.partial_hs.len()) >= smallest_known_size {
        // Prune this branch
    } else if instance.edges().is_empty() {
        // Instance is solved
        if state.partial_hs.len() < smallest_known_size {
            info!("Found HS of size {}", state.partial_hs.len());
            state.solution.minimum_hs.truncate(state.partial_hs.len());
            state.solution.minimum_hs.copy_from_slice(&state.partial_hs);
        } else {
            warn!(
                "Found HS is not smaller than best known ({} vs. {}), should have been pruned",
                state.partial_hs.len(),
                smallest_known_size,
            );
        }
    } else {
        // Branch on highest degree node
        let node = instance.max_node_degree().1;
        branch_on(node, instance, state);
    }

    reduction.restore(instance, &mut state.partial_hs);
}

pub fn solve<R: Rng + SeedableRng>(
    mut instance: Instance,
    file_name: String,
    seed: u64,
) -> Result<Solution> {
    let time_start = Instant::now();
    let mut base_hs = Vec::new();
    let initial_reduction = reductions::reduce(&mut instance, &mut base_hs, |_, _| false);
    let initial_reduction_runtime = time_start.elapsed();
    info!("Initial reduction time: {:.2?}", initial_reduction_runtime);

    let time_start = Instant::now();
    let greedy_hs = greedy_approx(&mut instance, base_hs);
    let greedy_size = greedy_hs.len();

    let mut state = State {
        rng: R::seed_from_u64(seed),
        partial_hs: Vec::new(),
        solution: Solution {
            file_name,
            minimum_hs: greedy_hs,
            seed,
            branching_steps: 0,
            greedy_size,
            runtime: Duration::default(),
            initial_reduction_runtime,
        },
        last_log_time: Instant::now(),
    };
    solve_recursive(&mut instance, &mut state);
    let runtime = Instant::now() - time_start;

    info!(
        "Solving took {} branching steps in {:.2?}",
        state.solution.branching_steps, runtime
    );
    debug!(
        "Final HS (size {}): {:?}",
        state.solution.minimum_hs.len(),
        &state.solution.minimum_hs
    );

    info!("Validating found hitting set");
    initial_reduction.restore(&mut instance, &mut state.partial_hs);
    let hs_set: IdxHashSet<_> = state.solution.minimum_hs.iter().copied().collect();
    assert_eq!(instance.num_nodes_total(), instance.nodes().len());
    assert_eq!(instance.num_edges_total(), instance.edges().len());
    for &edge_idx in instance.edges() {
        let hit = instance
            .edge(edge_idx)
            .any(|node_idx| hs_set.contains(&node_idx));
        assert!(hit, "edge {} not hit", edge_idx);
    }

    Ok(state.solution)
}
