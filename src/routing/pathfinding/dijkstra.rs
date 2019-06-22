use super::Graph;
use std:: {
    cmp::Ordering,
    collections::BinaryHeap,
};

//--------------------------------------------------------------------------------------------------
// nodes

#[derive(Copy, Clone)]
struct CostNode {
    pub id: usize,
    pub cost: f64,
}

impl Ord for CostNode {
    fn cmp(&self, other: &CostNode) -> Ordering {
        // (1) cost in float, but cmp uses only m, which is ok
        // (2) inverse order since BinaryHeap is max-heap, but min-heap is needed
        let delta = (other.cost - self.cost) as i64;
        delta.cmp(&0)
            .then_with(|| other.id.cmp(&self.id))
    }
}

impl PartialOrd for CostNode {
    fn partial_cmp(&self, other: &CostNode) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for CostNode {}

impl PartialEq for CostNode {
    fn eq(&self, other: &CostNode) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

//--------------------------------------------------------------------------------------------------
// Dijkstra's type of path

pub struct Path {
    pub cost: Vec<f64>,
    pub predecessors: Vec<usize>,
}

//--------------------------------------------------------------------------------------------------
// Dijkstra

pub struct Dijkstra<'a> {
    pub graph: &'a Graph,
    pub path: Path,
}

impl<'a> Dijkstra<'a> {
    pub fn new(graph: &'a Graph) -> Dijkstra {
        Dijkstra {
            graph,
            path: Path {
                cost: vec![std::f64::MAX; graph.node_count],
                predecessors: vec![std::usize::MAX; graph.node_count],
            }
        }
    }
}

impl<'a> Dijkstra<'a> {
    pub fn compute_shortest_path(&mut self, src: usize, dst: usize) {
        self.path.cost[src] = 0.0;
        let mut queue = BinaryHeap::new();
        queue.push(CostNode {cost: 0.0, id: src});
        while let Some(CostNode {cost, id} ) = queue.pop() {
            if id == dst {
                break;
            }
            if cost > self.path.cost[id] {
                continue;
            }
            let graph_node = &self.graph.nodes[id];
            for i in graph_node.edge_start .. graph_node.edge_end + 1 {
                let current_edge = &self.graph.edges[i];
                let current_cost = cost + current_edge.weight;
                if current_cost < self.path.cost[current_edge.dst] {
                    self.path.predecessors[current_edge.dst] = i;
                    self.path.cost[current_edge.dst] = current_cost;
                    queue.push(CostNode {cost: current_cost, id: current_edge.dst});
                }
            }
        }
    }

    pub fn get_distance(&mut self, node_id: usize) -> f64 {
        if node_id >= self.graph.node_count {
            let result = std::f64::MAX;
            result
        } else {
            self.path.cost[node_id]
        }
    }

    pub fn get_path(&mut self, src: usize, dst: usize) -> std::vec::Vec<usize> {
        if src >= self.graph.node_count || dst >= self.graph.node_count {
            let result = vec![];
            result
        } else {
            let mut shortest_path = Vec::new();
            let mut current_predec = dst;
            while current_predec != src {
                let current_edge = &self.graph.edges[self.path.predecessors[current_predec]];
                shortest_path.push(current_edge.id);
                current_predec = current_edge.src;
            }
            shortest_path
        }
    }
}
