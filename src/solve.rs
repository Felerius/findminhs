use crate::{
    instance::{Instance, NodeIdx},
    lower_bound::{self, PackingBound},
    reductions::{self, ReductionResult},
    report::{ReductionStats, Report, RootBounds, RuntimeStats, Settings},
    small_indices::IdxHashSet,
};
use log::{debug, info, trace, warn};
use std::time::Instant;

const ITERATION_LOG_INTERVAL_SECS: u64 = 60;

#[derive(Debug, Clone)]
struct State {
    partial_hs: Vec<NodeIdx>,
    minimum_hs: Vec<NodeIdx>,
    report: Report,
    last_log_time: Instant,
}

fn branch_on(node: NodeIdx, instance: &mut Instance, state: &mut State) {
    trace!("Branching on {}", node);
    state.report.branching_steps += 1;
    instance.delete_node(node);

    instance.delete_incident_edges(node);
    state.partial_hs.push(node);
    solve_recursive(instance, state);
    debug_assert_eq!(state.partial_hs.last().copied(), Some(node));
    state.partial_hs.pop();
    instance.restore_incident_edges(node);

    solve_recursive(instance, state);

    instance.restore_node(node);
}

fn solve_recursive(instance: &mut Instance, state: &mut State) {
    let now = Instant::now();
    if (now - state.last_log_time).as_secs() >= ITERATION_LOG_INTERVAL_SECS {
        info!(
            "Running on {} for {} branching steps",
            &state.report.file_name, state.report.branching_steps
        );
        state.last_log_time = now;
    }

    let (reduction_result, reduction) = reductions::reduce(
        instance,
        &mut state.partial_hs,
        &mut state.minimum_hs,
        &mut state.report.runtimes,
        &mut state.report.reductions,
        &state.report.settings,
    );
    match reduction_result {
        ReductionResult::Solved => {
            if state.partial_hs.len() < state.minimum_hs.len() {
                info!("Found HS of size {} by branching", state.partial_hs.len());
                state.minimum_hs.clear();
                state.minimum_hs.extend(state.partial_hs.iter().copied());
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
            branch_on(node, instance, state);
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
    };
    report
        .reductions
        .costly_discard_packing_from_scratch_steps_per_run =
        vec![0; packing_from_scratch_limit + 1];

    let time_start = Instant::now();
    let mut state = State {
        partial_hs: Vec::new(),
        minimum_hs: instance.nodes().to_vec(),
        report,
        last_log_time: time_start,
    };
    solve_recursive(&mut instance, &mut state);
    state.report.runtimes.total = time_start.elapsed();
    state.report.opt = state.minimum_hs.len();

    info!(
        "Solving took {} branching steps in {:.2?}",
        state.report.branching_steps, state.report.runtimes.total
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

    state.report
}
