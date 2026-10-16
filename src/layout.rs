//! Các thuật toán bố trí thuần, tái lập và không phụ thuộc giao diện terminal.

use crate::graph::Graph;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub type Point = [f64; 2];

/// Khung nhìn trong world-space đã được fit vào canvas Braille.
///
/// Một ô terminal chứa 2×4 chấm nên không thể coi ô ký tự là pixel vuông.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FitView {
    pub center: Point,
    pub half_size: Point,
    pub source_bounds: [f64; 4],
}

impl FitView {
    pub fn bounds(self, zoom: f64) -> ([f64; 2], [f64; 2]) {
        let zoom = zoom.max(0.05);
        let half_x = self.half_size[0] / zoom;
        let half_y = self.half_size[1] / zoom;
        (
            [self.center[0] - half_x, self.center[0] + half_x],
            [self.center[1] - half_y, self.center[1] + half_y],
        )
    }
}

/// Căn giữa và fit các node đang hiện vào 90% lưới chấm Braille.
///
/// Hàm thuần này là nguồn sự thật chung cho cả ba layout và có thể test mà
/// không cần dựng terminal.
pub fn fit_to_view(
    points: &[Point],
    visible: &BTreeSet<usize>,
    width_cells: u16,
    height_cells: u16,
) -> FitView {
    let selected: Vec<Point> = visible
        .iter()
        .filter_map(|index| points.get(*index).copied())
        .filter(|point| point[0].is_finite() && point[1].is_finite())
        .collect();
    let mut source = if selected.is_empty() {
        [-1.0, 1.0, -1.0, 1.0]
    } else {
        let mut value = [
            selected[0][0],
            selected[0][0],
            selected[0][1],
            selected[0][1],
        ];
        for point in &selected[1..] {
            value[0] = value[0].min(point[0]);
            value[1] = value[1].max(point[0]);
            value[2] = value[2].min(point[1]);
            value[3] = value[3].max(point[1]);
        }
        value
    };
    // Một điểm không có kích thước toán học. Nới chiều phẳng/đồ thị một node
    // để scale luôn hữu hạn và node không dính sát biên.
    let natural = ((source[1] - source[0])
        .abs()
        .max((source[3] - source[2]).abs())
        * 0.05)
        .max(1.0);
    if (source[1] - source[0]).abs() < f64::EPSILON {
        source[0] -= natural;
        source[1] += natural;
    }
    if (source[3] - source[2]).abs() < f64::EPSILON {
        source[2] -= natural;
        source[3] += natural;
    }
    let span_x = source[1] - source[0];
    let span_y = source[3] - source[2];
    let dots_x = f64::from(width_cells.max(1)) * 2.0;
    let dots_y = f64::from(height_cells.max(1)) * 4.0;
    let scale = ((dots_x * 0.9) / span_x)
        .min((dots_y * 0.9) / span_y)
        .max(f64::MIN_POSITIVE);
    FitView {
        center: [(source[0] + source[1]) / 2.0, (source[2] + source[3]) / 2.0],
        half_size: [dots_x / scale / 2.0, dots_y / scale / 2.0],
        source_bounds: source,
    }
}

#[derive(Debug, Clone)]
pub struct LayeredLayout {
    pub positions: Vec<Point>,
    pub levels: Vec<Option<usize>>,
}

/// Bố trí phân tầng bằng BFS và sắp barycenter nhiều vòng.
///
/// Router được ưu tiên làm gốc, sau đó đến switch L3 và node bậc cao nhất.
/// Các thành phần không nối được với gốc được đưa xuống hàng cuối.
pub fn layered(graph: &Graph, visible: &BTreeSet<usize>) -> LayeredLayout {
    let mut positions = vec![[0.0, 0.0]; graph.nodes.len()];
    let mut levels = vec![None; graph.nodes.len()];
    let root = visible
        .iter()
        .copied()
        .find(|index| graph.nodes[*index].kind.eq_ignore_ascii_case("router"))
        .or_else(|| {
            visible.iter().copied().find(|index| {
                graph.nodes[*index]
                    .kind
                    .eq_ignore_ascii_case("multilayerswitch")
            })
        })
        .or_else(|| {
            visible
                .iter()
                .copied()
                .max_by_key(|index| (graph.degree(*index), std::cmp::Reverse(*index)))
        });
    if let Some(root) = root {
        levels[root] = Some(0);
        let mut queue = VecDeque::from([root]);
        while let Some(node) = queue.pop_front() {
            let next = levels[node].unwrap_or(0) + 1;
            for neighbor in graph.neighbors(node) {
                if visible.contains(&neighbor) && levels[neighbor].is_none() {
                    levels[neighbor] = Some(next);
                    queue.push_back(neighbor);
                }
            }
        }
    }
    let connected_max = levels.iter().flatten().copied().max().unwrap_or(0);
    for index in visible {
        if levels[*index].is_none() {
            levels[*index] = Some(connected_max + 1);
        }
    }
    let mut rows: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for index in visible {
        if let Some(level) = levels[*index] {
            rows.entry(level).or_default().push(*index);
        }
    }
    for row in rows.values_mut() {
        row.sort_unstable();
    }
    for _ in 0..6 {
        let previous_order: BTreeMap<usize, f64> = rows
            .values()
            .flat_map(|row| {
                row.iter()
                    .enumerate()
                    .map(|(order, node)| (*node, order as f64))
            })
            .collect();
        for (level, row) in &mut rows {
            if *level == 0 || *level > connected_max {
                continue;
            }
            row.sort_by(|left, right| {
                let barycenter = |node: usize| {
                    let parents: Vec<_> = graph
                        .neighbors(node)
                        .filter(|neighbor| levels[*neighbor] == Some(level - 1))
                        .filter_map(|neighbor| previous_order.get(&neighbor).copied())
                        .collect();
                    if parents.is_empty() {
                        previous_order.get(&node).copied().unwrap_or(node as f64)
                    } else {
                        parents.iter().sum::<f64>() / parents.len() as f64
                    }
                };
                barycenter(*left)
                    .total_cmp(&barycenter(*right))
                    .then_with(|| left.cmp(right))
            });
        }
    }
    for (level, row) in rows {
        let width = row.len().saturating_sub(1) as f64 * 36.0;
        for (order, node) in row.into_iter().enumerate() {
            // Một hàng có thể chứa hàng chục endpoint. Khoảng cách tầng lớn
            // giúp các hàng vẫn tách biệt sau phép fit theo tỷ lệ Braille 2×4.
            positions[node] = [order as f64 * 36.0 - width / 2.0, -(level as f64) * 300.0];
        }
    }
    LayeredLayout { positions, levels }
}

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
                // Một số file chỉ có tọa độ cho một phần thiết bị. Vòng tròn
                // fallback giữ kết quả xác định mà không phá tọa độ đã có.
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
    // Dịch cực nhỏ nhưng tái lập để hai node không trùng tuyệt đối do làm tròn.
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
