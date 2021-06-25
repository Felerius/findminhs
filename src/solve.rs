use crate::{
    instance::{Instance, NodeIdx},
    lower_bound::{self, PackingBound},
    reductions::{self, ReductionResult},
    report::{ReductionStats, Report, RootBounds, RuntimeStats, Settings, UpperBoundImprovement},
    small_indices::IdxHashSet,
};
use log::{debug, info, trace, warn};
use std::time::Instant;

const ITERATION_LOG_INTERVAL_SECS: u64 = 60;

#[derive(Debug, Clone)]
pub struct State {
    pub partial_hs: Vec<NodeIdx>,
    pub minimum_hs: Vec<NodeIdx>,
    pub solve_start_time: Instant,
    pub last_log_time: Instant,
}

fn branch_on(node: NodeIdx, instance: &mut Instance, state: &mut State, report: &mut Report) {
    trace!("Branching on {}", node);
    report.branching_steps += 1;
    instance.delete_node(node);

    instance.delete_incident_edges(node);
    state.partial_hs.push(node);
    solve_recursive(instance, state, report);
    debug_assert_eq!(state.partial_hs.last().copied(), Some(node));
    state.partial_hs.pop();
    instance.restore_incident_edges(node);

    solve_recursive(instance, state, report);

    instance.restore_node(node);
}

fn solve_recursive(instance: &mut Instance, state: &mut State, report: &mut Report) {
    let now = Instant::now();
    if (now - state.last_log_time).as_secs() >= ITERATION_LOG_INTERVAL_SECS {
        info!(
            "Running on {} for {} branching steps",
            &report.file_name, report.branching_steps
        );
        state.last_log_time = now;
    }

    let (reduction_result, reduction) = reductions::reduce(instance, state, report);
    match reduction_result {
        ReductionResult::Solved => {
            if state.partial_hs.len() < state.minimum_hs.len() {
                info!("Found HS of size {} by branching", state.partial_hs.len());
                state.minimum_hs.clear();
                state.minimum_hs.extend(state.partial_hs.iter().copied());
                report.upper_bound_improvements.push(UpperBoundImprovement {
                    new_bound: state.minimum_hs.len(),
                    branching_steps: report.branching_steps,
                    runtime: state.solve_start_time.elapsed(),
                });
            } else {
                warn!(
                    "Found HS is not smaller than best known ({} vs. {}), should have been pruned",
                    state.partial_hs.len(),
                    state.minimum_hs.len(),
                );
            }
        }
        ReductionResult::Unsolvable => {}
        ReductionResult::Finished => {
            let node = instance
                .nodes()
                .iter()
                .copied()
                .max_by_key(|&node| instance.node_degree(node))
                .expect("Branching on an empty instance");
            branch_on(node, instance, state, report);
        }
    }

    reduction.restore(instance, &mut state.partial_hs);
}

pub fn solve(mut instance: Instance, file_name: String, settings: Settings) -> Report {
    let num_nodes = instance.num_nodes_total();
    let root_packing = PackingBound::new(&instance, &settings);
    let root_bounds = RootBounds {
        max_degree: lower_bound::calc_max_degree_bound(&instance).unwrap_or(num_nodes),
        sum_degree: lower_bound::calc_sum_degree_bound(&instance),
        efficiency: lower_bound::calc_efficiency_bound(&instance)
            .0
            .round()
            .unwrap_or(num_nodes),
        packing: root_packing.bound(),
        sum_over_packing: root_packing.calc_sum_over_packing_bound(&instance),
        greedy_upper: reductions::calc_greedy_approximation(&instance).len(),
    };
    let packing_from_scratch_limit = settings.packing_from_scratch_limit;
    let mut report = Report {
        file_name,
        opt: instance.num_nodes_total(),
        branching_steps: 0,
        settings,
        root_bounds,
        runtimes: RuntimeStats::default(),
        reductions: ReductionStats::default(),
        upper_bound_improvements: Vec::new(),
    };
    report
        .reductions
        .costly_discard_packing_from_scratch_steps_per_run =
        vec![0; packing_from_scratch_limit + 1];

    let mut state = State {
        partial_hs: Vec::new(),
        minimum_hs: instance.nodes().to_vec(),
        last_log_time: Instant::now(),
        solve_start_time: Instant::now(),
    };
    solve_recursive(&mut instance, &mut state, &mut report);
    report.runtimes.total = state.solve_start_time.elapsed();
    report.opt = state.minimum_hs.len();

    info!(
        "Solving took {} branching steps in {:.2?}",
        report.branching_steps, report.runtimes.total
    );
    debug!(
        "Final HS (size {}): {:?}",
        state.minimum_hs.len(),
        &state.minimum_hs
    );

    info!("Validating found hitting set");
    let hs_set: IdxHashSet<_> = state.minimum_hs.iter().copied().collect();
    assert_eq!(instance.num_nodes_total(), instance.nodes().len());
    assert_eq!(instance.num_edges_total(), instance.edges().len());
    for &edge in instance.edges() {
        let hit = instance.edge(edge).any(|node| hs_set.contains(&node));
        assert!(hit, "edge {} not hit", edge);
    }

    report
}
