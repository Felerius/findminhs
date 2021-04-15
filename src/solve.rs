use crate::{
    instance::{Instance, NodeIdx},
    lower_bound,
    reductions::{self, ReductionResult},
    small_indices::IdxHashSet,
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

    /// Initial lower bound
    pub lower_bound: usize,

    /// Size of the greedily found initial hitting set
    pub upper_bound: usize,

    /// How often the branching algorithm has branched
    pub branching_steps: usize,

    /// Total time required to solve the instance
    #[serde(serialize_with = "serialize_duration_as_seconds")]
    pub runtime: Duration,

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
    let now = Instant::now();
    if (now - state.last_log_time).as_secs() >= ITER_LOG_GAP {
        info!(
            "Running on {} for {} branching steps",
            &state.solution.file_name, state.solution.branching_steps
        );
        state.last_log_time = now;
    }

    let (reduction_result, reduction) = reductions::reduce(
        instance,
        &mut state.partial_hs,
        &mut state.solution.minimum_hs,
    );
    match reduction_result {
        ReductionResult::Solved => {
            if state.partial_hs.len() < state.solution.minimum_hs.len() {
                info!("Found HS of size {} by branching", state.partial_hs.len());
                state.solution.minimum_hs.clear();
                state
                    .solution
                    .minimum_hs
                    .extend(state.partial_hs.iter().copied());
            } else {
                warn!(
                    "Found HS is not smaller than best known ({} vs. {}), should have been pruned",
                    state.partial_hs.len(),
                    state.solution.minimum_hs.len(),
                );
            }
        }
        ReductionResult::Unsolvable => {}
        ReductionResult::Finished => {
            let node = instance
                .nodes()
                .iter()
                .copied()
                .max_by_key(|&node_idx| instance.node_degree(node_idx))
                .expect("Branching on an empty instance");
            branch_on(node, instance, state);
        }
    }

    reduction.restore(instance, &mut state.partial_hs);
}

pub fn solve<R: Rng + SeedableRng>(
    mut instance: Instance,
    file_name: String,
    seed: u64,
) -> Result<Solution> {
    let greedy_hs = reductions::greedy_approx(&instance);
    let (packing, _) = lower_bound::pack_edges(&instance);
    let lower_bound = lower_bound::calculate(&instance, &packing, 0);
    info!("Lower bound: {}", lower_bound);
    info!("Upper bound: {}", greedy_hs.len());

    let time_start = Instant::now();
    let mut state = State {
        rng: R::seed_from_u64(seed),
        partial_hs: Vec::new(),
        solution: Solution {
            file_name,
            minimum_hs: instance.nodes().to_vec(),
            seed,
            branching_steps: 0,
            lower_bound,
            upper_bound: greedy_hs.len(),
            runtime: Duration::default(),
        },
        last_log_time: Instant::now(),
    };
    solve_recursive(&mut instance, &mut state);
    state.solution.runtime = time_start.elapsed();

    info!(
        "Solving took {} branching steps in {:.2?}",
        state.solution.branching_steps, state.solution.runtime
    );
    debug!(
        "Final HS (size {}): {:?}",
        state.solution.minimum_hs.len(),
        &state.solution.minimum_hs
    );

    info!("Validating found hitting set");
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
