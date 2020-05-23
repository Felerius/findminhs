use crate::create_idx_struct;
use crate::data_structures::cont_idx_vec::ContiguousIdxVec;
use crate::data_structures::segtree::{SegTree, SegTreeOp};
use crate::data_structures::skipvec::SkipVec;
use crate::small_indices::{SmallIdx, IdxHashSet};
use anyhow::{anyhow, ensure, Result};
use log::{info, trace};
use std::io::BufRead;
use std::mem;
use std::time::Instant;

create_idx_struct!(NodeIdx);
create_idx_struct!(EdgeIdx);
create_idx_struct!(EntryIdx);

struct NodeDegreeOp;

#[derive(Clone, Debug)]
pub struct Instance {
    nodes: ContiguousIdxVec<NodeIdx>,
    edges: ContiguousIdxVec<EdgeIdx>,
    node_incidences: Vec<SkipVec<(EdgeIdx, EntryIdx)>>,
    edge_incidences: Vec<SkipVec<(NodeIdx, EntryIdx)>>,
    degree_1_edges: IdxHashSet<EdgeIdx>,
    node_degrees: SegTree<NodeDegreeOp>,
}

impl SegTreeOp for NodeDegreeOp {
    type Item = u32;
    type Lazy = ();

    fn apply(_: &mut Self::Item, _: Option<&mut Self::Lazy>, _: &Self::Lazy) {}

    fn combine(left: &Self::Item, right: &Self::Item) -> Self::Item {
        *left.max(right)
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
        let mut node_degrees = vec![0; num_nodes];
        for _ in 0..num_edges {
            line.clear();
            reader.read_line(&mut line)?;
            let mut numbers = line.split_ascii_whitespace().map(str::parse::<usize>);
            let degree = numbers
                .next()
                .ok_or_else(|| anyhow!("empty edge line in input, expected degree"))??;
            ensure!(degree > 0, "edges may not be empty");

            let incidences =
                SkipVec::try_sorted_from(numbers.map(|num_result| {
                    num_result.map(|num| (NodeIdx::from(num), EntryIdx::INVALID))
                }))?;
            for (_, (node_idx, _)) in &incidences {
                node_degrees[node_idx.idx()] += 1;
            }
            edge_incidences.push(incidences);
        }

        let mut node_incidences: Vec<_> = node_degrees
            .iter()
            .map(|&len| SkipVec::with_len(len))
            .collect();
        let mut rem_node_degrees = node_degrees;
        for (edge_idx, incidences) in edge_incidences.iter_mut().enumerate() {
            let edge_idx = EdgeIdx::from(edge_idx);
            for (edge_entry_idx, edge_entry) in incidences.iter_mut() {
                let node_idx = edge_entry.0.idx();
                let node_entry_idx = node_incidences[node_idx].len() - rem_node_degrees[node_idx];
                rem_node_degrees[node_idx] -= 1;
                edge_entry.1 = EntryIdx::from(node_entry_idx);
                node_incidences[node_idx][node_entry_idx] =
                    (edge_idx, EntryIdx::from(edge_entry_idx));
            }
        }

        let degree_1_edges = (0..num_edges).filter(|&idx| edge_incidences[idx].len() == 1).map(EdgeIdx::from).collect();
        let node_degrees = (0..num_nodes).map(|idx| node_incidences[idx].len() as u32).collect();

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
            degree_1_edges,
            node_degrees,
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
    ) -> impl Iterator<Item = EdgeIdx> + ExactSizeIterator + Clone + 'a {
        self.node_incidences[node_idx.idx()]
            .iter()
            .map(|(_, (edge_idx, _))| *edge_idx)
    }

    /// Nodes incident to an edge, sorted by increasing indices.
    pub fn edge<'a>(
        &'a self,
        edge_idx: EdgeIdx,
    ) -> impl Iterator<Item = NodeIdx> + ExactSizeIterator + Clone + 'a {
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

    pub fn max_node_degree(&self) -> usize {
        *self.node_degrees.root() as usize
    }

    pub fn degree_1_edge(&mut self) -> Option<(EdgeIdx, NodeIdx)> {
        self.degree_1_edges.iter().next().map(|&edge_idx| (edge_idx, self.edge(edge_idx).next().expect("Empty degree 1 edge")))
    }

    /// Deletes a node from the instance.
    pub fn delete_node(&mut self, node_idx: NodeIdx) {
        trace!("Deleting node {}", node_idx);
        for (_idx, (edge_idx, entry_idx)) in &self.node_incidences[node_idx.idx()] {
            self.edge_incidences[edge_idx.idx()].delete(entry_idx.idx());
            match self.edge_degree(*edge_idx) {
                0 => { self.degree_1_edges.remove(edge_idx); }
                1 => { self.degree_1_edges.insert(*edge_idx); }
                _ => {}
            }
        }
        self.nodes.delete(node_idx.idx());
        self.node_degrees.set(node_idx.idx(), 0);
    }

    /// Deletes an edge from the instance.
    pub fn delete_edge(&mut self, edge_idx: EdgeIdx) {
        trace!("Deleting edge {}", edge_idx);
        for (_idx, (node_idx, entry_idx)) in &self.edge_incidences[edge_idx.idx()] {
            self.node_incidences[node_idx.idx()].delete(entry_idx.idx());
            self.node_degrees.change(node_idx.idx(), |val| *val -= 1);
        }
        self.edges.delete(edge_idx.idx());
        if self.edge_degree(edge_idx) == 1 {
            self.degree_1_edges.remove(&edge_idx);
        }
    }

    /// Restores a previously deleted node.
    ///
    /// All restore operations (node or edge) must be done in reverse order of
    /// the corresponding deletions to produce sensible results.
    pub fn restore_node(&mut self, node_idx: NodeIdx) {
        trace!("Restoring node {}", node_idx);
        for (_idx, (edge_idx, entry_idx)) in self.node_incidences[node_idx.idx()].iter().rev() {
            self.edge_incidences[edge_idx.idx()].restore(entry_idx.idx());
            match self.edge_degree(*edge_idx) {
                1 => { self.degree_1_edges.insert(*edge_idx); }
                2 => { self.degree_1_edges.remove(edge_idx); }
                _ => {},
            }
        }
        self.nodes.restore(node_idx.idx());
        self.node_degrees.set(node_idx.idx(), self.node_degree(node_idx) as u32);
    }

    /// Restores a previously deleted edge.
    ///
    /// All restore operations (node or edge) must be done in reverse order of
    /// the corresponding deletions to produce sensible results.
    pub fn restore_edge(&mut self, edge_idx: EdgeIdx) {
        trace!("Restoring edge {}", edge_idx);
        for (_idx, (node_idx, entry_idx)) in self.edge_incidences[edge_idx.idx()].iter().rev() {
            self.node_incidences[node_idx.idx()].restore(entry_idx.idx());
            self.node_degrees.change(node_idx.idx(), |val| *val += 1);
        }
        self.edges.restore(edge_idx.idx());
        if self.edge_degree(edge_idx) == 1 {
            self.degree_1_edges.insert(edge_idx);
        }
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
