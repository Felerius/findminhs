use serde::{Deserialize, Serialize, Serializer};
use std::time::Duration;

fn serialize_duration_as_seconds<S>(duration: &Duration, ser: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    ser.serialize_f64(duration.as_secs_f64())
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RuntimeStats {
    #[serde(serialize_with = "serialize_duration_as_seconds")]
    pub total: Duration,

    #[serde(serialize_with = "serialize_duration_as_seconds")]
    pub greedy: Duration,

    #[serde(serialize_with = "serialize_duration_as_seconds")]
    pub max_degree_bound: Duration,

    #[serde(serialize_with = "serialize_duration_as_seconds")]
    pub sum_degree_bound: Duration,

    #[serde(serialize_with = "serialize_duration_as_seconds")]
    pub efficiency_bound: Duration,

    #[serde(serialize_with = "serialize_duration_as_seconds")]
    pub packing_bound: Duration,

    #[serde(serialize_with = "serialize_duration_as_seconds")]
    pub sum_over_packing_bound: Duration,

    #[serde(serialize_with = "serialize_duration_as_seconds")]
    pub forced_vertex: Duration,

    #[serde(serialize_with = "serialize_duration_as_seconds")]
    pub costly_discard_packing_update: Duration,

    #[serde(serialize_with = "serialize_duration_as_seconds")]
    pub costly_discard_packing_from_scratch: Duration,

    #[serde(serialize_with = "serialize_duration_as_seconds")]
    pub vertex_domination: Duration,

    #[serde(serialize_with = "serialize_duration_as_seconds")]
    pub edge_domination: Duration,

    #[serde(serialize_with = "serialize_duration_as_seconds")]
    pub applying_reductions: Duration,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ReductionStats {
    pub max_degree_bound_breaks: usize,
    pub sum_degree_bound_breaks: usize,
    pub efficiency_degree_bound_breaks: usize,
    pub packing_bound_breaks: usize,
    pub sum_over_packing_bound_breaks: usize,

    pub greedy_runs: usize,
    pub greedy_bound_improvements: usize,
    pub forced_vertex_runs: usize,
    pub forced_vertices_found: usize,
    pub costly_discard_efficiency_runs: usize,
    pub costly_discard_efficiency_vertices_found: usize,
    pub costly_discard_packing_update_runs: usize,
    pub costly_discard_packing_update_vertices_found: usize,
    pub costly_discard_packing_from_scratch_runs: usize,
    pub costly_discard_packing_from_scratch_steps_per_run: Vec<usize>,
    pub vertex_dominations_runs: usize,
    pub vertex_dominations_vertices_found: usize,
    pub edge_dominations_runs: usize,
    pub edge_dominations_edges_found: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RootBounds {
    pub max_degree: usize,
    pub sum_degree: usize,
    pub efficiency: usize,
    pub packing: usize,
    pub sum_over_packing: usize,
    pub greedy_upper: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GreedyMode {
    Once,
    AlwaysBeforeBounds,
    AlwaysBeforeExpensiveReductions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Use local search to improve the packing bound
    pub enable_local_search: bool,

    /// Enable the max-degree bound
    pub enable_max_degree_bound: bool,

    /// Enable the sum-degree bound
    pub enable_sum_degree_bound: bool,

    /// Enable the efficiency bound (including costly discards)
    pub enable_efficiency_bound: bool,

    /// Enable the packing bound (including costly discards)
    pub enable_packing_bound: bool,

    /// Enable the sum-over-packing bound (requires packing bound to be enabled)
    pub enable_sum_over_packing_bound: bool,

    /// Number of nodes to check in the costly discard with from-scratch packing step
    pub packing_from_scratch_limit: usize,

    /// When to update the greedy upper bound during reductions
    pub greedy_mode: GreedyMode,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub file_name: String,
    pub opt: usize,
    pub branching_steps: usize,
    pub settings: Settings,
    pub root_bounds: RootBounds,
    pub runtimes: RuntimeStats,
    pub reductions: ReductionStats,
}
