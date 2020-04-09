use crate::data_structures::segtree::{SegTree, SegTreeOp};
use crate::instance::{Instance, NodeIdx};
use anyhow::Result;
use log::trace;
use rand::{Rng, SeedableRng};
use std::cmp::Ordering;
use std::hint::unreachable_unchecked;

struct ActivitySegTreeOp<R>(R);

/// Activities which differ by less than this are considered equal
const ACTIVITY_EQ_EPSILON: f32 = 0.000_001;

/// Factor by which activities are decayed.
const ACTIVITY_DECAY_FACTOR: f32 = 0.99;

/// Implements a segment tree for activities.
///
/// It can:
///   * Report the node with the maximum activity. If the activities of two
///     nodes differs by less than `ACTIVITY_EQ_EPSILON`, they are considered
///     equal and the reported one is chosen randomly using an rng.
///   * Decay all activities by a factor.
///   * Add activity to a node.
/// All operations work in O(log n), where n is the number of nodes.
impl<R: Rng> SegTreeOp for ActivitySegTreeOp<R> {
    /// Contains the activity and the index of the node.
    ///
    /// The node index is set to `NodeIdx::INVALID` if the node has been
    /// deleted. This is respected in `combine` below to make sure that a
    /// deleted node is never reported as the maximum. At the same time, this
    /// still allows the node to receive activity boosts/decays just as normal.
    type Item = (f32, NodeIdx);
    type Lazy = f32;

    fn apply(&mut self, item: &mut Self::Item, lazy: Option<&mut Self::Lazy>, upper: &Self::Lazy) {
        item.0 *= upper;
        if let Some(lazy) = lazy {
            *lazy *= upper;
        }
    }

    fn combine(&mut self, left: &Self::Item, right: &Self::Item) -> Self::Item {
        if left.1 == NodeIdx::INVALID {
            *right
        } else if right.1 == NodeIdx::INVALID {
            *left
        } else if (left.0 - right.0).abs() < ACTIVITY_EQ_EPSILON {
            if self.0.gen() {
                *left
            } else {
                *right
            }
        } else {
            // We only ever add and multiply with constants, so we should never
            // have any NaN's. Check in debug mode, optimize release mode under
            // the above assumption.
            match left.0.partial_cmp(&right.0) {
                None => {
                    if cfg!(debug) {
                        panic!("Activity value was set to NaN")
                    } else {
                        unsafe { unreachable_unchecked() }
                    }
                }
                Some(Ordering::Less) | Some(Ordering::Equal) => *right,
                Some(Ordering::Greater) => *left,
            }
        }
    }

    fn no_lazy() -> Self::Lazy {
        0.0
    }
}

pub struct Activities<R: Rng> {
    activities: SegTree<ActivitySegTreeOp<R>>,
}

impl<R: Rng> Activities<R> {
    pub fn new(instance: &Instance, seed_rng: impl Rng) -> Result<Self>
    where
        R: SeedableRng,
    {
        let op = ActivitySegTreeOp(R::from_rng(seed_rng)?);
        let num_nodes = instance.num_nodes_total();
        let activities = SegTree::from_iter(
            op,
            (0..num_nodes).map(NodeIdx::from).map(|idx| {
                if instance.is_node_deleted(idx) {
                    (0.0, NodeIdx::INVALID)
                } else {
                    (0.0, idx)
                }
            }),
        );
        Ok(Self { activities })
    }

    pub fn decay_all(&mut self) {
        trace!("Decaying all");
        self.activities.apply_to_all(&ACTIVITY_DECAY_FACTOR);
    }

    pub fn boost_activity(&mut self, node_idx: NodeIdx, amount: f32) {
        trace!("Boosting {} by {}", node_idx, amount);
        self.activities
            .change_single(node_idx.idx(), |entry| entry.0 += amount);
    }

    pub fn delete(&mut self, node_idx: NodeIdx) {
        self.activities.change_single(node_idx.idx(), |entry| {
            debug_assert!(
                entry.1 != NodeIdx::INVALID,
                "Node {} was deleted twice",
                node_idx
            );
            entry.1 = NodeIdx::INVALID;
        });
    }

    pub fn restore(&mut self, node_idx: NodeIdx) {
        self.activities.change_single(node_idx.idx(), |entry| {
            debug_assert!(
                entry.1 == NodeIdx::INVALID,
                "Node {} restored without being deleted",
                node_idx
            );
            entry.1 = node_idx;
        });
    }

    pub fn highest(&self) -> NodeIdx {
        self.activities.root().1
    }
}
