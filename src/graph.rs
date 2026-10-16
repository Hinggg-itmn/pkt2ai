//! Terminal-independent topology graph model.

use crate::{Device, Info, Link};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug, Clone)]
pub struct Graph {
    pub nodes: Vec<Device>,
    pub edges: Vec<Edge>,
    index: BTreeMap<String, usize>,
    adjacency: Vec<BTreeSet<usize>>,
}

#[derive(Debug, Clone)]
pub struct Edge {
    pub link: Link,
    pub a: Option<usize>,
    pub b: Option<usize>,
    pub parallel_index: usize,
    pub parallel_count: usize,
}

impl Graph {
    pub fn new(info: &Info) -> Self {
        let nodes = info.devices.clone();
        let index: BTreeMap<_, _> = nodes
            .iter()
            .enumerate()
            .map(|(i, node)| (node.name.clone(), i))
            .collect();
        let mut pair_counts: BTreeMap<(usize, usize), usize> = BTreeMap::new();
        let mut edges = Vec::with_capacity(info.links.len());
        for link in &info.links {
            let a = index.get(&link.a).copied();
            let b = index.get(&link.b).copied();
            let pair = a.zip(b).map(|(a, b)| if a <= b { (a, b) } else { (b, a) });
            let parallel_index = pair
                .map(|pair| {
                    let value = pair_counts.entry(pair).or_default();
                    let current = *value;
                    *value += 1;
                    current
                })
                .unwrap_or(0);
            edges.push(Edge {
                link: link.clone(),
                a,
                b,
                parallel_index,
                parallel_count: 1,
            });
        }
        for edge in &mut edges {
            if let Some((a, b)) = edge.a.zip(edge.b) {
                edge.parallel_count = pair_counts[&(a.min(b), a.max(b))];
            }
        }
        let mut adjacency = vec![BTreeSet::new(); nodes.len()];
        for edge in &edges {
            if let Some((a, b)) = edge.a.zip(edge.b) {
                adjacency[a].insert(b);
                adjacency[b].insert(a);
            }
        }
        Self {
            nodes,
            edges,
            index,
            adjacency,
        }
    }

    pub fn node_index(&self, name: &str) -> Option<usize> {
        self.index.get(name).copied()
    }
    pub fn neighbors(&self, node: usize) -> impl Iterator<Item = usize> + '_ {
        self.adjacency.get(node).into_iter().flatten().copied()
    }
    pub fn degree(&self, node: usize) -> usize {
        self.adjacency.get(node).map_or(0, BTreeSet::len)
    }

    pub fn local_nodes(&self, start: usize, depth: usize) -> BTreeSet<usize> {
        if start >= self.nodes.len() {
            return BTreeSet::new();
        }
        let mut visible = BTreeSet::from([start]);
        let mut queue = VecDeque::from([(start, 0usize)]);
        while let Some((node, distance)) = queue.pop_front() {
            if distance == depth {
                continue;
            }
            for neighbor in self.neighbors(node) {
                if visible.insert(neighbor) {
                    queue.push_back((neighbor, distance + 1));
                }
            }
        }
        visible
    }

    pub fn visible_nodes(
        &self,
        kinds: &BTreeSet<String>,
        show_isolated: bool,
        local: Option<(usize, usize)>,
    ) -> BTreeSet<usize> {
        let local = local.map(|(node, depth)| self.local_nodes(node, depth));
        self.nodes
            .iter()
            .enumerate()
            .filter(|(index, node)| {
                (kinds.is_empty() || kinds.contains(&node.kind))
                    && (show_isolated || self.degree(*index) != 0)
                    && local.as_ref().is_none_or(|nodes| nodes.contains(index))
            })
            .map(|(index, _)| index)
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeStyle {
    pub symbol: char,
    pub color: &'static str,
}

pub fn node_style(kind: &str) -> NodeStyle {
    match kind.to_ascii_lowercase().replace([' ', '-'], "").as_str() {
        "router" => NodeStyle {
            symbol: '◆',
            color: "#ff9f43",
        },
        "switch" => NodeStyle {
            symbol: '■',
            color: "#54a0ff",
        },
        "multilayerswitch" => NodeStyle {
            symbol: '⬢',
            color: "#5f8cff",
        },
        "pc" => NodeStyle {
            symbol: '●',
            color: "#1dd1a1",
        },
        "laptop" => NodeStyle {
            symbol: '●',
            color: "#10ac84",
        },
        "server" => NodeStyle {
            symbol: '▣',
            color: "#ee5253",
        },
        "accesspoint" => NodeStyle {
            symbol: '▲',
            color: "#feca57",
        },
        "printer" => NodeStyle {
            symbol: '▤',
            color: "#c8d6e5",
        },
        _ => NodeStyle {
            symbol: '○',
            color: "#a4b0be",
        },
    }
}
