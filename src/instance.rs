use crate::create_idx_struct;
use crate::data_structures::cont_idx_vec::ContiguousIdxVec;
use crate::data_structures::segtree::{SegTree, SegTreeOp};
use crate::data_structures::skipvec::SkipVec;
use anyhow::{anyhow, ensure, Result};
use log::{info, trace};
use std::io::BufRead;
use std::mem;
use std::time::Instant;

create_idx_struct!(NodeIdx);
create_idx_struct!(EdgeIdx);
create_idx_struct!(EntryIdx);

struct EdgeDegreeOp;

pub struct Instance {
    nodes: ContiguousIdxVec<NodeIdx>,
    edges: ContiguousIdxVec<EdgeIdx>,
    node_incidences: Vec<SkipVec<(EdgeIdx, EntryIdx)>>,
    edge_incidences: Vec<SkipVec<(NodeIdx, EntryIdx)>>,
    edge_degrees: SegTree<EdgeDegreeOp>,
}

impl SegTreeOp for EdgeDegreeOp {
    // Track the edge with the maximum degree and an edge with degree 1, if it
    // exists
    type Item = (u32, EdgeIdx);
    type Lazy = ();

    fn apply(_: &mut Self::Item, _: Option<&mut Self::Lazy>, _: &Self::Lazy) {}

    fn combine(left: &Self::Item, right: &Self::Item) -> Self::Item {
        let deg_1_edge = if left.1.valid() { left.1 } else { right.1 };
        (left.0.max(right.0), deg_1_edge)
    }

    fn no_lazy() -> Self::Lazy {}
}

impl Instance {
    pub fn load(mut reader: impl BufRead) -> Result<Self> {
        let time_before = Instant::now();
        let mut line = String::new();

        reader.read_line(&mut line)?;
        let mut numbers = line.split_ascii_whitespace().map(str::parse);
        let num_nodes = numbers
            .next()
            .ok_or_else(|| anyhow!("Missing node count"))??;
        let num_edges = numbers
            .next()
            .ok_or_else(|| anyhow!("Missing edge count"))??;
        ensure!(
            numbers.next().is_none(),
            "Too many numbers in first input line"
        );

        let nodes = (0..num_nodes).map(NodeIdx::from).collect();
        let edges = (0..num_edges).map(EdgeIdx::from).collect();

        let mut edge_incidences = Vec::with_capacity(num_edges);
        for _ in 0..num_edges {
            line.clear();
            reader.read_line(&mut line)?;
            let mut numbers = line.split_ascii_whitespace().map(str::parse);
            let degree = numbers
                .next()
                .ok_or_else(|| anyhow!("empty edge line in input, expected degree"))??;
            ensure!(degree > 0, "edges may not be empty");

            let mut incidences =
                SkipVec::with_len(degree as usize, (NodeIdx::INVALID, EntryIdx::INVALID));
            for (node_idx, (_index, entry)) in numbers.zip(&mut incidences) {
                entry.0 = NodeIdx(node_idx?);
            }
            edge_incidences.push(incidences);
        }

        let mut all_incidences: Vec<_> = edge_incidences
            .iter()
            .enumerate()
            .flat_map(|(index, list)| {
                let edge_idx = EdgeIdx(index as u32);
                list.iter()
                    .map(move |(index, _value)| (edge_idx, EntryIdx::from(index)))
            })
            .collect();
        // Due to the double indirection, caching the key here is a bit faster
        // (and a lot faster than unstable sorts). Using a stable sort allows
        // us to avoid using the edge index as a secondary sort criteria.
        all_incidences.sort_by_cached_key(|(edge_idx, entry_idx)| {
            edge_incidences[edge_idx.idx()][entry_idx.idx()]
        });

        let mut node_incidences = Vec::with_capacity(num_nodes);
        let mut rem_incidences = &all_incidences[..];
        for node_idx in (0..num_nodes).map(NodeIdx::from) {
            let degree = rem_incidences
                .iter()
                .take_while(|(edge_idx, entry_idx)| {
                    edge_incidences[edge_idx.idx()][entry_idx.idx()].0 == node_idx
                })
                .count();
            let mut incidences = SkipVec::with_len(degree, (EdgeIdx::INVALID, EntryIdx::INVALID));

            // Patterns cannot be combined until https://github.com/rust-lang/rust/issues/68354
            // is stable
            for (rem_incidences_pair, incidences_pair) in
                rem_incidences[..degree].iter().zip(&mut incidences)
            {
                let (edge_idx, edge_entry_idx) = rem_incidences_pair;
                let (node_entry_idx, node_entry) = incidences_pair;
                edge_incidences[edge_idx.idx()][edge_entry_idx.idx()].1 =
                    EntryIdx::from(node_entry_idx);
                *node_entry = (*edge_idx, *edge_entry_idx);
            }
            node_incidences.push(incidences);
            rem_incidences = &rem_incidences[degree..];
        }

        let edge_degrees = (0..num_edges)
            .map(|idx| {
                let degree = edge_incidences[idx].len() as u32;
                let deg_1_edge = if degree == 1 {
                    EdgeIdx::from(idx)
                } else {
                    EdgeIdx::INVALID
                };
                (degree, deg_1_edge)
            })
            .collect();

        info!(
            "Loaded instance with {} nodes, {} edges in {:.2?}",
            num_nodes,
            num_edges,
            Instant::now() - time_before,
        );
        Ok(Self {
            nodes,
            edges,
            node_incidences,
            edge_incidences,
            edge_degrees,
        })
    }

    pub fn num_nodes(&self) -> usize {
        self.nodes.len()
    }

    pub fn num_edges(&self) -> usize {
        self.edges.len()
    }

    pub fn num_nodes_total(&self) -> usize {
        self.node_incidences.len()
    }

    /// Edges incident to a node, sorted by increasing indices.
    pub fn node<'a>(
        &'a self,
        node_idx: NodeIdx,
    ) -> impl Iterator<Item = EdgeIdx> + ExactSizeIterator + 'a {
        self.node_incidences[node_idx.idx()]
            .iter()
            .map(|(_, (edge_idx, _))| *edge_idx)
    }

    /// Nodes incident to an edge, sorted by increasing indices.
    pub fn edge<'a>(
        &'a self,
        edge_idx: EdgeIdx,
    ) -> impl Iterator<Item = NodeIdx> + ExactSizeIterator + 'a {
        self.edge_incidences[edge_idx.idx()]
            .iter()
            .map(|(_, (node_idx, _))| *node_idx)
    }

    /// Alive nodes in the instance, in arbitrary order.
    pub fn nodes(&self) -> &[NodeIdx] {
        &self.nodes
    }

    pub fn is_node_deleted(&self, node_idx: NodeIdx) -> bool {
        self.nodes.is_deleted(node_idx.idx())
    }

    /// Alive edges in the instance, in arbitrary order.
    pub fn edges(&self) -> &[EdgeIdx] {
        &self.edges
    }

    pub fn node_degree(&self, node_idx: NodeIdx) -> usize {
        self.node_incidences[node_idx.idx()].len()
    }

    pub fn edge_degree(&self, edge_idx: EdgeIdx) -> usize {
        self.edge_incidences[edge_idx.idx()].len()
    }

    pub fn max_edge_degree(&self) -> usize {
        self.edge_degrees.root().0 as usize
    }

    pub fn degree_1_edge(&self) -> Option<EdgeIdx> {
        if self.edge_degrees.root().1.valid() {
            Some(self.edge_degrees.root().1)
        } else {
            None
        }
    }

    /// Deletes a node from the instance.
    pub fn delete_node(&mut self, node_idx: NodeIdx) {
        trace!("Deleting node {}", node_idx);
        for (_idx, (edge_idx, entry_idx)) in &self.node_incidences[node_idx.idx()] {
            self.edge_incidences[edge_idx.idx()].delete(entry_idx.idx());
            self.edge_degrees.change_single(edge_idx.idx(), |item| {
                item.0 -= 1;
                item.1 = if item.0 == 1 {
                    *edge_idx
                } else {
                    EdgeIdx::INVALID
                };
            })
        }
        self.nodes.delete(node_idx.idx());
    }

    /// Deletes an edge from the instance.
    pub fn delete_edge(&mut self, edge_idx: EdgeIdx) {
        trace!("Deleting edge {}", edge_idx);
        for (_idx, (node_idx, entry_idx)) in &self.edge_incidences[edge_idx.idx()] {
            self.node_incidences[node_idx.idx()].delete(entry_idx.idx());
        }
        self.edges.delete(edge_idx.idx());
        self.edge_degrees
            .change_single(edge_idx.idx(), |item| *item = (0, EdgeIdx::INVALID))
    }

    /// Restores a previously deleted node.
    ///
    /// All restore operations (node or edge) must be done in reverse order of
    /// the corresponding deletions to produce sensible results.
    pub fn restore_node(&mut self, node_idx: NodeIdx) {
        trace!("Restoring node {}", node_idx);
        for (_idx, (edge_idx, entry_idx)) in self.node_incidences[node_idx.idx()].iter().rev() {
            self.edge_incidences[edge_idx.idx()].restore(entry_idx.idx());
            self.edge_degrees.change_single(edge_idx.idx(), |item| {
                item.0 += 1;
                item.1 = if item.0 == 1 {
                    *edge_idx
                } else {
                    EdgeIdx::INVALID
                };
            })
        }
        self.nodes.restore(node_idx.idx());
    }

    /// Restores a previously deleted edge.
    ///
    /// All restore operations (node or edge) must be done in reverse order of
    /// the corresponding deletions to produce sensible results.
    pub fn restore_edge(&mut self, edge_idx: EdgeIdx) {
        trace!("Restoring edge {}", edge_idx);
        for (_idx, (node_idx, entry_idx)) in self.edge_incidences[edge_idx.idx()].iter().rev() {
            self.node_incidences[node_idx.idx()].restore(entry_idx.idx());
        }
        self.edges.restore(edge_idx.idx());
        let degree = self.edge_degree(edge_idx) as u32;
        self.edge_degrees.change_single(edge_idx.idx(), |item| {
            item.0 = degree;
            item.1 = if degree == 1 {
                edge_idx
            } else {
                EdgeIdx::INVALID
            };
        })
    }

    /// Deletes all edges incident to a node.
    ///
    /// The node itself must have already been deleted.
    pub fn delete_incident_edges(&mut self, node_idx: NodeIdx) {
        // We want to iterate over the incidence of `node_idx` while deleting
        // edges, which in turn changes node incidences. This is safe, since
        // `node_idx` itself was already deleted. To make the borrow checker
        // accept this, we temporarily move `node_idx` incidence to a local
        // variable, replacing it with an empty list. This should not be much
        // slower than unsafe alternatives, since an incidence list is only
        // 28 bytes large.
        trace!("Deleting all edges incident to {}", node_idx);
        debug_assert!(
            self.nodes.is_deleted(node_idx.idx()),
            "Node passed to delete_incident_edges must be deleted"
        );
        let incidence = mem::take(&mut self.node_incidences[node_idx.idx()]);
        for (_, (edge_idx, _)) in &incidence {
            self.delete_edge(*edge_idx);
        }
        self.node_incidences[node_idx.idx()] = incidence;
    }

    /// Restores all incident edges to a node.
    ///
    /// This reverses the effect of `delete_incident_edges`. As with all other
    /// `restore_*` methods, this must be done in reverse order of deletions.
    /// In particular, the node itself must still be deleted.
    pub fn restore_incident_edges(&mut self, node_idx: NodeIdx) {
        trace!("Restoring all edges incident to {}", node_idx);
        debug_assert!(
            self.nodes.is_deleted(node_idx.idx()),
            "Node passed to restore_incident_edges must be deleted"
        );

        // See `delete_incident_edges` for an explanation of this swapping around
        let incidence = mem::take(&mut self.node_incidences[node_idx.idx()]);

        // It is important that we restore the edges in reverse order
        for (_, (edge_idx, _)) in incidence.iter().rev() {
            self.restore_edge(*edge_idx);
        }
        self.node_incidences[node_idx.idx()] = incidence;
    }
}
