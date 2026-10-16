//! Deterministic force-directed and Packet Tracer coordinate layouts.

use crate::graph::Graph;

pub type Point = [f64; 2];

#[derive(Clone, Copy)]
struct SplitMix64(u64);
impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

pub fn packet_tracer(graph: &Graph, seed: u64) -> Vec<Point> {
    let mut rng = SplitMix64(seed);
    let fallback_radius = (graph.nodes.len().max(1) as f64).sqrt() * 20.0;
    let count = graph.nodes.len().max(1) as f64;
    graph
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            node.xy.unwrap_or_else(|| {
                let angle = std::f64::consts::TAU * index as f64 / count + rng.unit() * 0.01;
                [angle.cos() * fallback_radius, angle.sin() * fallback_radius]
            })
        })
        .collect()
}

pub fn force_directed(graph: &Graph, seed: u64, iterations: usize) -> Vec<Point> {
    let n = graph.nodes.len();
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![[0.0, 0.0]];
    }
    let mut rng = SplitMix64(seed);
    let area = 10_000.0f64.max(n as f64 * 100.0);
    let k = (area / n as f64).sqrt();
    let mut positions: Vec<Point> = (0..n)
        .map(|_| [(rng.unit() - 0.5) * 100.0, (rng.unit() - 0.5) * 100.0])
        .collect();
    let mut displacement = vec![[0.0, 0.0]; n];
    for iteration in 0..iterations.max(1) {
        displacement.fill([0.0, 0.0]);
        for a in 0..n {
            for b in a + 1..n {
                let mut dx = positions[a][0] - positions[b][0];
                let mut dy = positions[a][1] - positions[b][1];
                let mut distance = dx.hypot(dy);
                if distance < 1e-9 {
                    let angle = std::f64::consts::TAU * rng.unit();
                    dx = angle.cos() * 1e-3;
                    dy = angle.sin() * 1e-3;
                    distance = 1e-3;
                }
                let force = k * k / distance;
                let fx = dx / distance * force;
                let fy = dy / distance * force;
                displacement[a][0] += fx;
                displacement[a][1] += fy;
                displacement[b][0] -= fx;
                displacement[b][1] -= fy;
            }
        }
        for edge in &graph.edges {
            let Some((a, b)) = edge.a.zip(edge.b) else {
                continue;
            };
            if a == b {
                continue;
            }
            let dx = positions[a][0] - positions[b][0];
            let dy = positions[a][1] - positions[b][1];
            let distance = dx.hypot(dy).max(1e-9);
            let force = distance * distance / k;
            let fx = dx / distance * force;
            let fy = dy / distance * force;
            displacement[a][0] -= fx;
            displacement[a][1] -= fy;
            displacement[b][0] += fx;
            displacement[b][1] += fy;
        }
        let temperature = 20.0 * (1.0 - iteration as f64 / iterations.max(1) as f64).max(0.02);
        for (position, delta) in positions.iter_mut().zip(&displacement) {
            let length = delta[0].hypot(delta[1]).max(1e-9);
            position[0] += delta[0] / length * length.min(temperature);
            position[1] += delta[1] / length * length.min(temperature);
        }
    }
    // A deterministic microscopic nudge guarantees distinct points after rounding errors.
    for index in 0..n {
        for earlier in 0..index {
            if positions[index] == positions[earlier] {
                positions[index][0] += (index + 1) as f64 * 1e-6;
            }
        }
    }
    positions
}

pub fn bounds(points: &[Point]) -> [f64; 4] {
    if points.is_empty() {
        return [-1.0, 1.0, -1.0, 1.0];
    }
    let mut result = [points[0][0], points[0][0], points[0][1], points[0][1]];
    for point in &points[1..] {
        result[0] = result[0].min(point[0]);
        result[1] = result[1].max(point[0]);
        result[2] = result[2].min(point[1]);
        result[3] = result[3].max(point[1]);
    }
    if (result[1] - result[0]).abs() < 1e-9 {
        result[0] -= 1.0;
        result[1] += 1.0;
    }
    if (result[3] - result[2]).abs() < 1e-9 {
        result[2] -= 1.0;
        result[3] += 1.0;
    }
    result
}
