use crate::data_structures::segtree::{SegTree, SegTreeOp};
use crate::instance::{Instance, NodeIdx};
use crate::small_indices::SmallIdx;
use anyhow::Result;
use log::trace;
use rand::{Rng, SeedableRng};
use std::cmp::Ordering;
use std::hint::unreachable_unchecked;

/// Activities which differ by less than this are considered equal
const ACTIVITY_EQ_EPSILON: f32 = 0.000_001;

/// Factor by which activities are decayed.
const ACTIVITY_DECAY_FACTOR: f32 = 0.99;

#[derive(Debug)]
struct ActivitySegTreeOp;

#[derive(Debug, Copy, Clone, Default)]
struct SegTreeItem {
    /// Activity of the associated node.
    #[cfg(not(feature = "split-activity"))]
    activity: f32,

    /// Activity of the associated node.
    #[cfg(feature = "split-activity")]
    activity: (f32, f32),

    /// Which node this item belongs to.
    ///
    /// This is set to `NodeIdx::INVALID` if the node has been deleted. This
    /// way a deleted node can still receive activity boosts without being
    /// considered for the node with the most activity.
    node_idx: NodeIdx,

    /// Random value used to tiebreak equal activities.
    ///
    /// This is rerolled whenever a nodes activity changes.
    tiebreak: u32,
}

fn choose(left: f32, left_tiebreak: u32, left_idx: NodeIdx, right: f32, right_tiebreak: u32, right_idx: NodeIdx) -> bool {
    if !left_idx.valid() {
        true
    } else if !right_idx.valid() {
        false
    } else if (left - right).abs() < ACTIVITY_EQ_EPSILON {
        right_tiebreak > left_tiebreak
    } else {
        // We only ever add and multiply with constants, so we should never
        // have any NaN's. Check in debug mode, optimize release mode under
        // the above assumption.
        match left.partial_cmp(&right) {
            None => {
                if cfg!(debug) {
                    panic!("Activity value was set to NaN")
                } else {
                    unsafe { unreachable_unchecked() }
                }
            }
            Some(Ordering::Less) | Some(Ordering::Equal) => true,
            Some(Ordering::Greater) => false,
        }
    }
}

/// Implements a segment tree for activities.
///
/// It can:
///   * Report the node with the maximum activity. If the activities of two
///     nodes differs by less than `ACTIVITY_EQ_EPSILON`, they are considered
///     equal and the reported one is chosen randomly using an rng.
///   * Decay all activities by a factor.
///   * Add activity to a node.
/// All operations work in O(log n), where n is the number of nodes.
impl SegTreeOp for ActivitySegTreeOp {
    type Item = SegTreeItem;
    type Lazy = f32;

    #[cfg(not(feature = "split-activity"))]
    fn apply(item: &mut Self::Item, lazy: Option<&mut Self::Lazy>, upper: &Self::Lazy) {
        item.activity *= upper;
        if let Some(lazy) = lazy {
            *lazy *= upper;
        }
    }

    #[cfg(feature = "split-activity")]
    fn apply(item: &mut Self::Item, lazy: Option<&mut Self::Lazy>, upper: &Self::Lazy) {
        item.activity.0 *= upper;
        item.activity.1 *= upper;
        if let Some(lazy) = lazy {
            *lazy *= upper;
        }
    }

    #[cfg(not(feature = "split-activity"))]
    fn combine(left: &Self::Item, right: &Self::Item) -> Self::Item {
        if choose(left.activity, left.tiebreak, left.node_idx, right.activity, right.tiebreak, right.node_idx) {
            *right
        } else {
            *left
        }
    }

    #[cfg(feature = "split-activity")]
    fn combine(left: &Self::Item, right: &Self::Item) -> Self::Item {
        let (left_activity, right_activity) = if cfg!(feature = "split-activity-max") {
            (left.activity.0.max(left.activity.1), right.activity.0.max(right.activity.1))
        } else if cfg!(feature = "split-activity-sum") {
            (left.activity.0 + left.activity.1, right.activity.0 + right.activity.1)
        } else {
            panic!("No activity combinator chosen")
        };
        if choose(left_activity, left.tiebreak, left.node_idx, right_activity, right.tiebreak, right.node_idx) {
            *right
        } else {
            *left
        }
    }

    fn no_lazy() -> Self::Lazy {
        0.0
    }
}

#[derive(Debug, Clone)]
pub struct Activities<R: Rng> {
    activities: SegTree<ActivitySegTreeOp>,
    rng: R,
}

impl<R: Rng> Activities<R> {
    pub fn new(instance: &Instance, seed_rng: impl Rng) -> Result<Self>
    where
        R: SeedableRng,
    {
        let mut rng = R::from_rng(seed_rng)?;
        let activities = (0..instance.num_nodes_total())
            .map(NodeIdx::from)
            .map(|idx| {
                let node_idx = if instance.is_node_deleted(idx) {
                    NodeIdx::INVALID
                } else {
                    idx
                };
                SegTreeItem {
                    #[cfg(feature = "split-activity")]
                    activity: (0.0, 0.0),
                    #[cfg(not(feature = "split-activity"))]
                    activity: 0.0,
                    node_idx,
                    tiebreak: rng.gen(),
                }
            })
            .collect();
        Ok(Self { activities, rng })
    }

    pub fn decay_all(&mut self) {
        trace!("Decaying all");
        self.activities.apply_to_all(&ACTIVITY_DECAY_FACTOR);
    }

    #[cfg(not(feature = "split-activity"))]
    pub fn boost_activity(&mut self, node_idx: NodeIdx, amount: f32) {
        trace!("Boosting {} by {}", node_idx, amount);
        let new_tiebreak = self.rng.gen();
        self.activities.change(node_idx.idx(), |item| {
            item.activity += amount;
            item.tiebreak = new_tiebreak;
        });
    }

    #[cfg(feature = "split-activity")]
    pub fn boost_activity(&mut self, node_idx: NodeIdx, amount: (f32, f32)) {
        trace!("Boosting {} by {:?}", node_idx, amount);
        let new_tiebreak = self.rng.gen();
        self.activities.change(node_idx.idx(), |item| {
            item.activity.0 += amount.0;
            item.activity.1 += amount.1;
            item.tiebreak = new_tiebreak;
        });
    }

    pub fn delete(&mut self, node_idx: NodeIdx) {
        self.activities.change(node_idx.idx(), |item| {
            debug_assert!(item.node_idx.valid(), "Node {} was deleted twice", node_idx);
            item.node_idx = NodeIdx::INVALID;
        });
    }

    pub fn restore(&mut self, node_idx: NodeIdx) {
        self.activities.change(node_idx.idx(), |item| {
            debug_assert!(
                !item.node_idx.valid(),
                "Node {} restored without being deleted",
                node_idx
            );
            item.node_idx = node_idx;
        });
    }

    pub fn highest(&self) -> NodeIdx {
        self.activities.root().node_idx
    }
}
