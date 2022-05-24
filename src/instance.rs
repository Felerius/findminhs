use crate::{
    create_idx_struct,
    data_structures::{cont_idx_vec::ContiguousIdxVec, skipvec::SkipVec},
    small_indices::SmallIdx,
};
use anyhow::{anyhow, ensure, Error, Result};
use log::{info, trace};
use serde::Deserialize;
use std::{
    fmt::{self, Display, Write as _},
    io::{BufRead, Write},
    mem,
    time::Instant,
};

create_idx_struct!(pub NodeIdx);
create_idx_struct!(pub EdgeIdx);
create_idx_struct!(pub EntryIdx);

#[derive(Debug)]
struct CompressedIlpName<T>(T);

impl<T: SmallIdx> Display for CompressedIlpName<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        let mut val = self.0.idx();
        while val != 0 {
            f.write_char(char::from(CHARS[val % CHARS.len()]))?;
            val /= CHARS.len();
        }
        Ok(())
    }
}

#[derive(Debug)]
struct ParsedEdgeHandler {
    edge_incidences: Vec<SkipVec<(NodeIdx, EntryIdx)>>,
    node_degrees: Vec<usize>,
}

impl ParsedEdgeHandler {
    fn handle_edge(&mut self, node_indices: impl IntoIterator<Item = Result<usize>>) -> Result<()> {
        let incidences = SkipVec::try_sorted_from(node_indices.into_iter().map(|idx_result| {
            idx_result.and_then(|node_idx| {
                ensure!(
                    node_idx < self.node_degrees.len(),
                    "invalid node idx in edge: {}",
                    node_idx
                );
                Ok((NodeIdx::from(node_idx), EntryIdx::INVALID))
            })
        }))?;
        ensure!(incidences.len() > 0, "edges may not be empty");
        for (_, (node, _)) in &incidences {
            self.node_degrees[node.idx()] += 1;
        }
        self.edge_incidences.push(incidences);
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct JsonInstance {
    num_nodes: usize,
    edges: Vec<Vec<usize>>,
}

#[derive(Clone, Debug)]
pub struct Instance {
    nodes: ContiguousIdxVec<NodeIdx>,
    edges: ContiguousIdxVec<EdgeIdx>,
    node_incidences: Vec<SkipVec<(EdgeIdx, EntryIdx)>>,
    edge_incidences: Vec<SkipVec<(NodeIdx, EntryIdx)>>,
}

impl Instance {
    fn load(
        num_nodes: usize,
        num_edges: usize,
        read_edges: impl FnOnce(&mut ParsedEdgeHandler) -> Result<()>,
    ) -> Result<Self> {
        let mut handler = ParsedEdgeHandler {
            edge_incidences: Vec::with_capacity(num_edges),
            node_degrees: vec![0; num_nodes],
        };
        read_edges(&mut handler)?;
        let ParsedEdgeHandler {
            mut edge_incidences,
            node_degrees,
        } = handler;

        let mut node_incidences: Vec<_> = node_degrees
            .iter()
            .map(|&len| SkipVec::with_len(len))
            .collect();
        let mut rem_node_degrees = node_degrees;
        for (edge, incidences) in edge_incidences.iter_mut().enumerate() {
            let edge = EdgeIdx::from(edge);
            for (edge_entry_idx, edge_entry) in incidences.iter_mut() {
                let node = edge_entry.0.idx();
                let node_entry_idx = node_incidences[node].len() - rem_node_degrees[node];
                rem_node_degrees[node] -= 1;
                edge_entry.1 = EntryIdx::from(node_entry_idx);
                node_incidences[node][node_entry_idx] = (edge, EntryIdx::from(edge_entry_idx));
            }
        }

        Ok(Self {
            nodes: (0..num_nodes).map(NodeIdx::from).collect(),
            edges: (0..num_edges).map(EdgeIdx::from).collect(),
            node_incidences,
            edge_incidences,
        })
    }

    pub fn load_from_text(mut reader: impl BufRead) -> Result<Self> {
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

        let instance = Self::load(num_nodes, num_edges, |handler| {
            for _ in 0..num_edges {
                line.clear();
                reader.read_line(&mut line)?;
                let mut numbers = line
                    .split_ascii_whitespace()
                    .map(|s| s.parse::<usize>().map_err(Error::from));
                // Skip degree
                numbers
                    .next()
                    .ok_or_else(|| anyhow!("empty edge line in input, expected degree"))??;
                handler.handle_edge(numbers)?;
            }

            Ok(())
        })?;

        info!(
            "Loaded text instance with {} nodes, {} edges in {:.2?}",
            num_nodes,
            num_edges,
            time_before.elapsed(),
        );
        Ok(instance)
    }

    pub fn load_from_json(mut reader: impl BufRead) -> Result<Self> {
        let time_before = Instant::now();

        // Usually faster for large inputs, see https://github.com/serde-rs/json/issues/160
        let mut text = String::new();
        reader.read_to_string(&mut text)?;
        let JsonInstance { num_nodes, edges } = serde_json::from_str(&text)?;

        let num_edges = edges.len();
        let instance = Self::load(num_nodes, num_edges, |handler| {
            for edge in edges {
                handler.handle_edge(edge.into_iter().map(Ok))?;
            }
            Ok(())
        })?;

        info!(
            "Loaded json instance with {} nodes, {} edges in {:.2?}",
            num_nodes,
            num_edges,
            time_before.elapsed(),
        );
        Ok(instance)
    }

    pub fn num_edges(&self) -> usize {
        self.edges.len()
    }

    pub fn num_nodes_total(&self) -> usize {
        self.node_incidences.len()
    }

    pub fn num_edges_total(&self) -> usize {
        self.edge_incidences.len()
    }

    /// Edges incident to a node, sorted by increasing indices.
    pub fn node(
        &self,
        node: NodeIdx,
    ) -> impl Iterator<Item = EdgeIdx> + ExactSizeIterator + Clone + '_ {
        self.node_incidences[node.idx()]
            .iter()
            .map(|(_, (edge, _))| *edge)
    }

    /// Nodes incident to an edge, sorted by increasing indices.
    pub fn edge(
        &self,
        edge: EdgeIdx,
    ) -> impl Iterator<Item = NodeIdx> + ExactSizeIterator + Clone + '_ {
        self.edge_incidences[edge.idx()]
            .iter()
            .map(|(_, (node, _))| *node)
    }

    /// Alive nodes in the instance, in arbitrary order.
    pub fn nodes(&self) -> &[NodeIdx] {
        &self.nodes
    }

    /// Alive edges in the instance, in arbitrary order.
    pub fn edges(&self) -> &[EdgeIdx] {
        &self.edges
    }

    pub fn node_degree(&self, node: NodeIdx) -> usize {
        self.node_incidences[node.idx()].len()
    }

    pub fn edge_size(&self, edge: EdgeIdx) -> usize {
        self.edge_incidences[edge.idx()].len()
    }

    /// Deletes a node from the instance.
    pub fn delete_node(&mut self, node: NodeIdx) {
        trace!("Deleting node {}", node);
        for (_idx, (edge, entry_idx)) in &self.node_incidences[node.idx()] {
            self.edge_incidences[edge.idx()].delete(entry_idx.idx());
        }
        self.nodes.delete(node.idx());
    }

    /// Deletes an edge from the instance.
    pub fn delete_edge(&mut self, edge: EdgeIdx) {
        trace!("Deleting edge {}", edge);
        for (_idx, (node, entry_idx)) in &self.edge_incidences[edge.idx()] {
            self.node_incidences[node.idx()].delete(entry_idx.idx());
        }
        self.edges.delete(edge.idx());
    }

    /// Restores a previously deleted node.
    ///
    /// All restore operations (node or edge) must be done in reverse order of
    /// the corresponding deletions to produce sensible results.
    pub fn restore_node(&mut self, node: NodeIdx) {
        trace!("Restoring node {}", node);
        for (_idx, (edge, entry_idx)) in self.node_incidences[node.idx()].iter().rev() {
            self.edge_incidences[edge.idx()].restore(entry_idx.idx());
        }
        self.nodes.restore(node.idx());
    }

    /// Restores a previously deleted edge.
    ///
    /// All restore operations (node or edge) must be done in reverse order of
    /// the corresponding deletions to produce sensible results.
    pub fn restore_edge(&mut self, edge: EdgeIdx) {
        trace!("Restoring edge {}", edge);
        for (_idx, (node, entry_idx)) in self.edge_incidences[edge.idx()].iter().rev() {
            self.node_incidences[node.idx()].restore(entry_idx.idx());
        }
        self.edges.restore(edge.idx());
    }

    /// Deletes all edges incident to a node.
    ///
    /// The node itself must have already been deleted.
    pub fn delete_incident_edges(&mut self, node: NodeIdx) {
        // We want to iterate over the incidence of `node` while deleting
        // edges, which in turn changes node incidences. This is safe, since
        // `node` itself was already deleted. To make the borrow checker
        // accept this, we temporarily move `node` incidence to a local
        // variable, replacing it with an empty list. This should not be much
        // slower than unsafe alternatives, since an incidence list is only
        // 28 bytes large.
        trace!("Deleting all edges incident to {}", node);
        debug_assert!(
            self.nodes.is_deleted(node.idx()),
            "Node passed to delete_incident_edges must be deleted"
        );
        let incidence = mem::take(&mut self.node_incidences[node.idx()]);
        for (_, (edge, _)) in &incidence {
            self.delete_edge(*edge);
        }
        self.node_incidences[node.idx()] = incidence;
    }

    /// Restores all incident edges to a node.
    ///
    /// This reverses the effect of `delete_incident_edges`. As with all other
    /// `restore_*` methods, this must be done in reverse order of deletions.
    /// In particular, the node itself must still be deleted.
    pub fn restore_incident_edges(&mut self, node: NodeIdx) {
        trace!("Restoring all edges incident to {}", node);
        debug_assert!(
            self.nodes.is_deleted(node.idx()),
            "Node passed to restore_incident_edges must be deleted"
        );

        // See `delete_incident_edges` for an explanation of this swapping around
        let incidence = mem::take(&mut self.node_incidences[node.idx()]);

        // It is important that we restore the edges in reverse order
        for (_, (edge, _)) in incidence.iter().rev() {
            self.restore_edge(*edge);
        }
        self.node_incidences[node.idx()] = incidence;
    }

    pub fn export_as_ilp(&self, mut writer: impl Write) -> Result<()> {
        writeln!(writer, "Minimize")?;
        write!(writer, "  v{}", CompressedIlpName(self.nodes()[0]))?;
        for &node in &self.nodes()[1..] {
            write!(writer, " + v{}", CompressedIlpName(node))?;
        }
        writeln!(writer)?;

        writeln!(writer, "Subject To")?;
        for &edge in self.edges() {
            write!(writer, "  e{}: ", CompressedIlpName(edge))?;
            for (idx, node) in self.edge(edge).enumerate() {
                if idx > 0 {
                    write!(writer, " + ")?;
                }
                write!(writer, "v{}", CompressedIlpName(node))?;
            }
            writeln!(writer, " >= 1")?;
        }

        writeln!(writer, "Binaries")?;
        write!(writer, "  v{}", CompressedIlpName(self.nodes()[0]))?;
        for &node in &self.nodes()[1..] {
            write!(writer, " v{}", CompressedIlpName(node))?;
        }
        writeln!(writer)?;

        writeln!(writer, "End")?;
        Ok(())
    }
}
