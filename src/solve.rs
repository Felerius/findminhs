use crate::instance::{Instance, NodeIdx};
use log::{debug, info};
use rand::Rng;
use rand::seq::SliceRandom;


struct State<R> {
    rng: R,
    incomplete_hs: Vec<NodeIdx>,
    best_known: usize,
}

fn greedy_approx(instance: &mut Instance) -> usize {
    let mut hs = vec![];
    while !instance.edges().is_empty() {
        let mut max_degree = (0, NodeIdx::INVALID);
        for &node in instance.nodes() {
            max_degree = max_degree.max((instance.degree(node), node));
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
        debug!("Found HS of size {}: {:?}", state.incomplete_hs.len(), state.incomplete_hs);
        state.best_known = state.incomplete_hs.len();
    }
    if state.incomplete_hs.len() + 1 >= state.best_known || instance.nodes().is_empty() {
        return;
    }

    // unwrap is safe due to the check above
    let node = *instance.nodes().choose(&mut state.rng).unwrap();
    if state.rng.gen() {
        instance.delete_node(node);
        solve_recursive(instance, state);
        instance.delete_incident_edges(node);
        state.incomplete_hs.push(node);
        solve_recursive(instance, state);
        state.incomplete_hs.pop();
        instance.restore_incident_edges(node);
        instance.restore_node(node);
    } else {
        instance.delete_node(node);
        instance.delete_incident_edges(node);
        state.incomplete_hs.push(node);
        solve_recursive(instance, state);
        state.incomplete_hs.pop();
        instance.restore_incident_edges(node);
        solve_recursive(instance, state);
        instance.restore_node(node);
    }
}

pub fn solve(instance: &mut Instance, rng: impl Rng) {
    let approx = greedy_approx(instance);
    let mut state = State {
        rng,
        incomplete_hs: vec![],
        best_known: approx,
    };
    solve_recursive(instance, &mut state);
    info!("Smallest HS: {}", state.best_known);
}
