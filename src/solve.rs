use crate::activity::Activities;
use crate::instance::{Instance, NodeIdx};
use anyhow::Result;
use log::{debug, trace};
use rand::{Rng, SeedableRng};

struct State<R: Rng> {
    rng: R,
    incomplete_hs: Vec<NodeIdx>,
    best_known: usize,
    activities: Activities<R>,
}

fn greedy_approx(instance: &mut Instance) -> usize {
    let mut hs = vec![];
    while !instance.edges().is_empty() {
        let mut max_degree = (0, NodeIdx::INVALID);
        for &node in instance.nodes() {
            max_degree = max_degree.max((instance.node_degree(node), node));
        }
        instance.delete_node(max_degree.1);
        instance.delete_incident_edges(max_degree.1);
        hs.push(max_degree.1);
    }
    for &node in hs.iter().rev() {
        instance.restore_incident_edges(node);
        instance.restore_node(node);
    }
    debug!("Greedy hs of size {}: {:?}", hs.len(), hs);
    hs.len()
}

fn solve_recursive(instance: &mut Instance, state: &mut State<impl Rng>) {
    if instance.edges().is_empty() {
        debug!(
            "Found HS of size {}: {:?}",
            state.incomplete_hs.len(),
            state.incomplete_hs
        );
        state.best_known = state.incomplete_hs.len();
    }
    if state.incomplete_hs.len() + 1 >= state.best_known || instance.nodes().is_empty() {
        for &node in &state.incomplete_hs {
            state.activities.boost_activity(node, 1.0);
        }
        return;
    }

    // use rand::seq::SliceRandom;
    // let node = *instance.nodes().choose(&mut state.rng).unwrap();  // unwrap is safe due to the check above
    let node = state.activities.highest();
    trace!("Branching on {}", node);
    if state.rng.gen() {
        instance.delete_node(node);
        state.activities.delete(node);
        solve_recursive(instance, state);
        instance.delete_incident_edges(node);
        state.incomplete_hs.push(node);
        solve_recursive(instance, state);
        state.incomplete_hs.pop();
        instance.restore_incident_edges(node);
        instance.restore_node(node);
        state.activities.restore(node);
    } else {
        instance.delete_node(node);
        state.activities.delete(node);
        instance.delete_incident_edges(node);
        state.incomplete_hs.push(node);
        solve_recursive(instance, state);
        state.incomplete_hs.pop();
        instance.restore_incident_edges(node);
        solve_recursive(instance, state);
        instance.restore_node(node);
        state.activities.restore(node);
    }

    state.activities.decay_all();
}

pub fn solve(instance: &mut Instance, mut rng: impl Rng + SeedableRng) -> Result<usize> {
    let approx = greedy_approx(instance);
    let activities = Activities::new(instance, &mut rng)?;
    let mut state = State {
        rng,
        incomplete_hs: vec![],
        best_known: approx,
        activities,
    };
    solve_recursive(instance, &mut state);
    Ok(state.best_known)
}
