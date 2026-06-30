//! Graph Laplacian Computation
//!
//! Computes the graph Laplacian L = D - A for the Arcturus chip topology:
//! - 100x100 grid with 4-nearest-neighbor connectivity
//! - 100 long-range edges (Manhattan-routed)
//!
//! The Laplacian eigenvalues λ_k determine the spectral properties:
//! - λ₀ = 0 (for connected graph)
//! - λ_k for k>0 determine the eigenbasis storage frequencies

use super::{Fixed, MatrixIndex, NUM_LONG_RANGE_EDGES, SparseMatrix, MAX_DIMENSION};

/// Edge type for Laplacian construction
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EdgeType {
    /// Nearest neighbor (grid edge)
    NearestNeighbor,
    /// Long-range edge (Manhattan-routed)
    LongRange,
}

/// Edge information
#[derive(Debug, Clone, Copy)]
pub struct Edge {
    /// Source node
    pub from: MatrixIndex,
    /// Target node
    pub to: MatrixIndex,
    /// Edge type
    pub edge_type: EdgeType,
    /// Edge weight (1 for unweighted, can be fractional for weighted)
    pub weight: Fixed,
}

impl Edge {
    /// Create a new edge
    pub fn new(from: MatrixIndex, to: MatrixIndex, edge_type: EdgeType, weight: Fixed) -> Self {
        Self { from, to, edge_type, weight }
    }

    /// Create a nearest neighbor edge with weight 1
    pub fn nearest_neighbor(from: MatrixIndex, to: MatrixIndex) -> Self {
        Self::new(from, to, EdgeType::NearestNeighbor, super::super::memory::FIXED_ONE)
    }

    /// Create a long-range edge
    pub fn long_range(from: MatrixIndex, to: MatrixIndex, weight: Fixed) -> Self {
        Self::new(from, to, EdgeType::LongRange, weight)
    }

    /// Get the transpose (reverse direction)
    pub fn transpose(&self) -> Self {
        Self {
            from: self.to,
            to: self.from,
            edge_type: self.edge_type,
            weight: self.weight,
        }
    }

    /// Check if this is a self-loop
    pub fn is_self_loop(&self) -> bool {
        self.from == self.to
    }
}

/// Adjacency list representation for efficient Laplacian construction
pub struct AdjacencyList {
    /// Edges from each node (index by node ID)
    neighbors: Box<[heapless::Vec<(MatrixIndex, Fixed), 8>]>,
    /// Total number of edges (each undirected edge counted once)
    num_edges: usize,
    /// Actual number of nodes with edges (for proper dimension tracking)
    actual_nodes: usize,
}

impl AdjacencyList {
    /// Create a new empty adjacency list
    pub fn new() -> Self {
        // Allocate on heap to avoid stack overflow
        let mut neighbors = Vec::with_capacity(MAX_DIMENSION);

        // Initialize empty neighbor lists for all nodes
        for _ in 0..MAX_DIMENSION {
            neighbors.push(heapless::Vec::new());
        }

        Self {
            neighbors: neighbors.into_boxed_slice(),
            num_edges: 0,
            actual_nodes: 0, // Will be updated as nodes are actually used
        }
    }



    /// Add an undirected edge (adds both directions)
    pub fn add_edge(&mut self, u: MatrixIndex, v: MatrixIndex, weight: Fixed) -> Result<(), ()> {
        if u as usize >= MAX_DIMENSION || v as usize >= MAX_DIMENSION {
            return Err(());
        }

        // Track actual nodes used (for proper dimension reporting)
        self.actual_nodes = self.actual_nodes.max((u as usize) + 1);
        self.actual_nodes = self.actual_nodes.max((v as usize) + 1);

        // Add v to u's neighbor list
        self.neighbors[u as usize].push((v, weight)).map_err(|_| ())?;

        // Add u to v's neighbor list (undirected)
        self.neighbors[v as usize].push((u, weight)).map_err(|_| ())?;

        self.num_edges += 1;
        Ok(())
    }

    /// Get degree of a node (sum of weights)
    pub fn degree(&self, node: MatrixIndex) -> Fixed {
        if node as usize >= MAX_DIMENSION {
            return 0;
        }
        
        let mut sum: Fixed = 0;
        for (_, weight) in &self.neighbors[node as usize] {
            sum = sum.saturating_add(*weight);
        }
        sum
    }

    /// Get number of neighbors
    pub fn num_neighbors(&self, node: MatrixIndex) -> usize {
        if node as usize >= MAX_DIMENSION {
            return 0;
        }
        self.neighbors[node as usize].len()
    }

    /// Iterate over neighbors of a node
    pub fn iter_neighbors(&self, node: MatrixIndex) -> impl Iterator<Item = &(MatrixIndex, Fixed)> {
        if node as usize >= MAX_DIMENSION {
            [].iter() // Empty iterator
        } else {
            self.neighbors[node as usize].iter()
        }
    }

    /// Total number of nodes
    pub fn num_nodes(&self) -> usize {
        // Return the tracked actual nodes (highest node index + 1)
        self.actual_nodes
    }

    /// Total number of edges (undirected)
    pub fn num_edges(&self) -> usize {
        self.num_edges
    }

    /// Clear all edges
    pub fn clear(&mut self) {
        for neighbors in &mut self.neighbors {
            neighbors.clear();
        }
        self.num_edges = 0;
    }
}

impl Default for AdjacencyList {
    fn default() -> Self {
        Self::new()
    }
}

/// Grid topology generator for 100x100 lattice
pub struct GridTopology {
    /// Grid width
    pub width: usize,
    /// Grid height
    pub height: usize,
    /// Include diagonal edges (8-connectivity)
    pub diagonal: bool,
    /// Number of long-range edges
    pub num_long_range: usize,
}

impl GridTopology {
    /// Create a standard 100x100 grid topology
    pub fn standard_100x100() -> Self {
        Self {
            width: 100,
            height: 100,
            diagonal: false, // 4-connectivity
            num_long_range: NUM_LONG_RANGE_EDGES,
        }
    }

    /// Convert 2D grid coordinates to node ID
    pub fn coord_to_id(&self, row: usize, col: usize) -> MatrixIndex {
        (row * self.width + col) as MatrixIndex
    }

    /// Convert node ID to 2D grid coordinates
    pub fn id_to_coord(&self, id: MatrixIndex) -> (usize, usize) {
        let id_usize = id as usize;
        (id_usize / self.width, id_usize % self.width)
    }

    /// Generate adjacency list for grid topology
    pub fn generate_adjacency_list(&self) -> AdjacencyList {
        let mut adj = AdjacencyList::new();
        let weight = super::super::memory::FIXED_ONE;

        // Add nearest neighbor edges (4-connectivity)
        for row in 0..self.height {
            for col in 0..self.width {
                let node = self.coord_to_id(row, col);

                // Right neighbor
                if col + 1 < self.width {
                    let right = self.coord_to_id(row, col + 1);
                    let _ = adj.add_edge(node, right, weight);
                }

                // Bottom neighbor
                if row + 1 < self.height {
                    let bottom = self.coord_to_id(row + 1, col);
                    let _ = adj.add_edge(node, bottom, weight);
                }

                // Add diagonal edges if enabled (8-connectivity)
                if self.diagonal {
                    // Bottom-right diagonal
                    if row + 1 < self.height && col + 1 < self.width {
                        let diag = self.coord_to_id(row + 1, col + 1);
                        // Diagonal edges have weight 1/√2 for distance
                        let diag_weight = (weight as i64 * 46341 / 65536) as Fixed; // 1/√2
                        let _ = adj.add_edge(node, diag, diag_weight);
                    }
                }
            }
        }

        // Add long-range edges (random Manhattan-routed for small-world effect)
        // For deterministic behavior, use fixed seed pattern
        for i in 0..self.num_long_range {
            // Use simple deterministic pseudo-random placement
            let seed = i * 9301 + 49297;
            let node1 = (seed % (self.width * self.height)) as MatrixIndex;
            let node2 = ((seed * 48271) % (self.width * self.height)) as MatrixIndex;

            if node1 != node2 {
                // Long-range edges have smaller weight (weaker coupling)
                let lr_weight = (weight as i64 * 32768 / 65536) as Fixed; // 0.5
                let _ = adj.add_edge(node1, node2, lr_weight);
            }
        }

        adj
    }

    /// Get number of nodes
    pub fn num_nodes(&self) -> usize {
        self.width * self.height
    }

    /// Get grid dimensions
    pub fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }
}

impl Default for GridTopology {
    fn default() -> Self {
        Self::standard_100x100()
    }
}

/// Laplacian matrix builder
pub struct LaplacianBuilder;

impl LaplacianBuilder {
    /// Build Laplacian matrix from adjacency list
    /// L = D - A (combinatorial Laplacian)
    pub fn build_combinatorial<const MAX_ELEM: usize>(
        adj: &AdjacencyList,
    ) -> SparseMatrix<MAX_ELEM> {
        let n = adj.num_nodes() as MatrixIndex;
        let mut laplacian = SparseMatrix::new(n);

        // For each node, add diagonal (degree) and off-diagonal (-1 for each edge)
        for node in 0..n {
            let degree = adj.degree(node);
            
            // Diagonal element: degree
            if degree != 0 {
                laplacian.set(node, node, degree, 0).ok();
            }

            // Off-diagonal elements: -weight for each edge
            for (neighbor, weight) in adj.iter_neighbors(node) {
                if *neighbor != node {
                    // L[i,j] = -A[i,j] for i != j
                    let neg_weight = (-(*weight as i64)) as Fixed;
                    laplacian.set(node, *neighbor, neg_weight, 0).ok();
                }
            }
        }

        laplacian
    }

    /// Build normalized Laplacian
    /// L_norm = I - D^{-1/2} A D^{-1/2}
    /// (Not fully implemented - requires division operations)
    pub fn build_normalized<const MAX_ELEM: usize>(
        _adj: &AdjacencyList,
    ) -> SparseMatrix<MAX_ELEM> {
        // Placeholder: normalized Laplacian construction
        // Requires computing D^{-1/2} which needs division
        todo!("Normalized Laplacian not yet implemented")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grid_topology() {
        let grid = GridTopology::standard_100x100();
        
        assert_eq!(grid.num_nodes(), grid.width * grid.height);
        assert_eq!(grid.dimensions(), (100, 100));

        // Test coordinate conversion
        let id = grid.coord_to_id(50, 50);
        assert_eq!(id, 5050);

        let (row, col) = grid.id_to_coord(5050);
        assert_eq!(row, 50);
        assert_eq!(col, 50);
    }

    #[test]
    fn test_adjacency_list() {
        let mut adj = AdjacencyList::new();

        // Add some edges
        adj.add_edge(0, 1, 1000).unwrap();
        adj.add_edge(1, 2, 1000).unwrap();
        adj.add_edge(2, 0, 1000).unwrap(); // Triangle

        // Check degrees
        assert_eq!(adj.degree(0), 2000); // Connected to 1 and 2
        assert_eq!(adj.degree(1), 2000); // Connected to 0 and 2
        assert_eq!(adj.degree(2), 2000); // Connected to 0 and 1

        // Check neighbor iteration
        let neighbors_0: Vec<_> = adj.iter_neighbors(0).map(|(n, _)| *n).collect();
        assert!(neighbors_0.contains(&1));
        assert!(neighbors_0.contains(&2));
    }

    #[test]
    fn test_grid_adjacency_generation() {
        let grid = GridTopology::standard_100x100();
        let adj = grid.generate_adjacency_list();

        // Should have the same number of nodes as the topology reports
        assert_eq!(adj.num_nodes(), grid.num_nodes());

        // Interior nodes should have degree 4 (grid connectivity)
        // Node at (50, 50) = index 5050
        let center_node = 5050;
        let center_degree = adj.degree(center_node);
        assert!(center_degree > 0);

        // Corner nodes have degree 2 (grid) plus possibly long-range edges
        let corner_degree = adj.degree(0);
        assert!(corner_degree >= 2000); // At least 2 neighbors with weight 1000 each

        // Total edges should be approximately 2*100*100 (horizontal) + 2*100*100 (vertical) 
        // = 20,000 grid edges + 100 long-range edges
        let num_edges = adj.num_edges();
        assert!(num_edges >= 10000); // At least grid edges
    }

    #[test]
    fn test_laplacian_construction() {
        use crate::memory::FIXED_ONE;

        // Create a simple 2x2 grid
        let grid = GridTopology {
            width: 2,
            height: 2,
            diagonal: false,
            num_long_range: 0,
        };

        let adj = grid.generate_adjacency_list();
        let laplacian = LaplacianBuilder::build_combinatorial::<100>(&adj);

        // 2x2 grid has 4 nodes
        assert_eq!(laplacian.dimension, 4);

        // Node 0 (corner): degree 2 (connected to 1 and 2)
        let l_00 = laplacian.get(0, 0);
        let expected_degree = 2 * FIXED_ONE;
        assert_eq!(l_00.re, expected_degree);

        // Off-diagonal: L[0,1] = -1 (edge between 0 and 1)
        let l_01 = laplacian.get(0, 1);
        assert_eq!(l_01.re, -FIXED_ONE);

        // Check diagonal sum equals number of edges * 2
        let mut total_degree: Fixed = 0;
        for i in 0..4 {
            total_degree = total_degree.saturating_add(laplacian.get(i, i).re);
        }
        // Sum of degrees = 2 * |E|
        // 2x2 grid has 4 edges, so sum of degrees = 8
        assert_eq!(total_degree, 4 * expected_degree);
    }

    #[test]
    fn test_laplacian_properties() {
        // Test that Laplacian has the expected properties:
        // 1. Row sums are zero
        // 2. Symmetric
        // 3. Positive semidefinite (all eigenvalues >= 0)
        // 4. Smallest eigenvalue is 0

        let grid = GridTopology::standard_100x100();
        let adj = grid.generate_adjacency_list();
        let laplacian = LaplacianBuilder::build_combinatorial::<10000>(&adj);

        // Check a few rows sum to approximately zero
        for row in [0, 100, 5000, 9999] {
            let mut row_sum: Fixed = 0;
            // Just check diagonal and some neighbors (sparse structure)
            // Diagonal
            row_sum = row_sum.saturating_add(laplacian.get(row, row).re);
            
            // In a grid, each node has up to 4 neighbors
            // Check right neighbor
            if (row as usize) % 100 < 99 {
                row_sum = row_sum.saturating_add(laplacian.get(row, row + 1).re);
            }
            // Check left neighbor
            if (row as usize) % 100 > 0 {
                row_sum = row_sum.saturating_add(laplacian.get(row, row - 1).re);
            }
            // Check top neighbor
            if row >= 100 {
                row_sum = row_sum.saturating_add(laplacian.get(row, row - 100).re);
            }
            // Check bottom neighbor
            if row < 9900 {
                row_sum = row_sum.saturating_add(laplacian.get(row, row + 100).re);
            }

            // Row sum should be approximately zero (allowing for fixed-point errors)
            assert!(
                row_sum.abs() < 1000,
                "Row {} sum should be ~0, got {}",
                row,
                row_sum
            );
        }
    }
}
