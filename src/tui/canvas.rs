//! Chuyển graph và viewport thành canvas Braille có màu, cạnh và nhãn.
//!
//! Module này giữ riêng việc lập kế hoạch nhãn để có thể kiểm thử va chạm
//! bằng `TestBackend`, thay vì phụ thuộc vào cảm quan trên terminal thật.

use crate::{
    graph::Graph,
    layout::{FitView, Point},
};
use ratatui::{
    style::{Color, Style},
    symbols::Marker,
    text::Line as TextLine,
    widgets::{
        Block, Borders,
        canvas::{Canvas, Line, Points},
    },
};
use std::collections::BTreeSet;

/// Ánh xạ loại thiết bị sang palette ANSI 16 màu.
///
/// `emphasized = false` chỉ giảm độ sáng, không đưa node về màu gần nền.
pub fn type_color(kind: &str, emphasized: bool) -> Color {
    let normalized = kind.to_ascii_lowercase().replace([' ', '-'], "");
    match (normalized.as_str(), emphasized) {
        ("router", true) => Color::LightYellow,
        ("router", false) => Color::Yellow,
        ("switch", true) => Color::LightBlue,
        ("switch", false) => Color::Blue,
        ("multilayerswitch", true) => Color::LightCyan,
        ("multilayerswitch", false) => Color::Cyan,
        ("pc", true) => Color::LightGreen,
        ("pc", false) => Color::Green,
        // Giữ Laptop khác màu MultiLayerSwitch ở trạng thái không chọn node.
        ("laptop", true | false) => Color::Cyan,
        ("server", true) => Color::LightRed,
        ("server", false) => Color::Red,
        ("accesspoint", true) => Color::LightMagenta,
        ("accesspoint", false) => Color::Magenta,
        ("printer", true) => Color::White,
        ("printer", false) => Color::Gray,
        (_, true) => Color::LightYellow,
        (_, false) => Color::Yellow,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub fit: FitView,
    /// Tỷ lệ tương đối so với fit; 1.0 chính là 100% fit-to-view.
    pub zoom: f64,
    pub pan: Point,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            fit: FitView {
                center: [0.0, 0.0],
                half_size: [2.0, 2.0],
                source_bounds: [-1.0, 1.0, -1.0, 1.0],
            },
            zoom: 1.0,
            pan: [0.0, 0.0],
        }
    }
}

impl Viewport {
    pub fn bounds(self) -> ([f64; 2], [f64; 2]) {
        let mut fit = self.fit;
        fit.center[0] += self.pan[0];
        fit.center[1] += self.pan[1];
        fit.bounds(self.zoom)
    }

    pub fn reset_fit(&mut self, fit: FitView) {
        self.fit = fit;
        self.zoom = 1.0;
        self.pan = [0.0, 0.0];
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedLabel {
    pub node: usize,
    pub text: String,
    pub cell_x: i32,
    pub cell_y: i32,
    pub cell_width: i32,
}

fn short_label(name: &str) -> String {
    let count = name.chars().count();
    if count <= 14 {
        name.to_owned()
    } else {
        format!("{}…", name.chars().take(13).collect::<String>())
    }
}

fn overlaps(left: &PlannedLabel, right: &PlannedLabel) -> bool {
    left.cell_y == right.cell_y
        && left.cell_x < right.cell_x + right.cell_width
        && right.cell_x < left.cell_x + left.cell_width
}

/// Chọn nhãn theo độ ưu tiên và đặt tham lam trên lưới ô ký tự.
///
/// Node chọn và hàng xóm là bắt buộc; hub và nhãn bổ sung có thể bị bỏ nếu
/// không còn vị trí không giao nhau.
pub fn plan_labels(
    graph: &Graph,
    positions: &[Point],
    visible: &BTreeSet<usize>,
    selected: Option<usize>,
    show_extra: bool,
    viewport: Viewport,
    size: [u16; 2],
) -> Vec<PlannedLabel> {
    let (x_bounds, y_bounds) = viewport.bounds();
    let width = i32::from(size[0].max(1));
    let height = i32::from(size[1].max(1));
    let neighbors: BTreeSet<_> = selected
        .into_iter()
        .flat_map(|node| graph.neighbors(node))
        .collect();
    let mut candidates: Vec<(u8, usize)> = visible
        .iter()
        .copied()
        .filter(|index| *index < positions.len())
        .filter_map(|index| {
            let priority = if selected == Some(index) {
                0
            } else if neighbors.contains(&index) {
                1
            } else if graph.degree(index) >= 3 {
                2
            } else if show_extra && viewport.zoom >= 1.35 {
                3
            } else {
                return None;
            };
            Some((priority, index))
        })
        .collect();
    candidates.sort_by_key(|(priority, index)| {
        (*priority, std::cmp::Reverse(graph.degree(*index)), *index)
    });
    let mut placed: Vec<PlannedLabel> = Vec::new();
    for (priority, index) in candidates {
        let point = positions[index];
        let node_x = ((point[0] - x_bounds[0]) / (x_bounds[1] - x_bounds[0]) * f64::from(width - 1))
            .round() as i32;
        let node_y = ((y_bounds[1] - point[1]) / (y_bounds[1] - y_bounds[0])
            * f64::from(height - 1))
        .round() as i32;
        let text = short_label(&graph.nodes[index].name);
        let label_width = text.chars().count() as i32;
        let choices = [
            (node_x + 2, node_y),
            (node_x - label_width - 2, node_y),
            (node_x - label_width / 2, node_y.saturating_sub(1)),
            (node_x - label_width / 2, node_y + 1),
        ];
        let is_free = |x: i32, y: i32, placed: &[PlannedLabel]| {
            x >= 0
                && y >= 0
                && x + label_width <= width
                && y < height
                && !placed.iter().any(|other| {
                    overlaps(
                        other,
                        &PlannedLabel {
                            node: index,
                            text: String::new(),
                            cell_x: x,
                            cell_y: y,
                            cell_width: label_width,
                        },
                    )
                })
        };
        let selected_choice = choices.into_iter().find(|(x, y)| is_free(*x, *y, &placed));
        // Node chọn và hàng xóm là bắt buộc. Khi bốn vị trí gần node đều bận,
        // tìm ô trống gần nhất thay vì bỏ nhãn hoặc cho hai nhãn đè nhau.
        let mandatory_fallback = || {
            (0..height)
                .flat_map(|y| (0..=(width - label_width).max(0)).map(move |x| (x, y)))
                .filter(|(x, y)| is_free(*x, *y, &placed))
                .min_by_key(|(x, y)| (x - node_x).abs() + (y - node_y).abs())
        };
        if let Some((cell_x, cell_y)) = selected_choice.or_else(|| {
            if priority <= 1 {
                mandatory_fallback()
            } else {
                None
            }
        }) {
            let label = PlannedLabel {
                node: index,
                text,
                cell_x,
                cell_y,
                cell_width: label_width,
            };
            placed.push(label);
        }
    }
    placed
}

pub fn widget<'a>(
    graph: &'a Graph,
    positions: &'a [Point],
    visible: &'a BTreeSet<usize>,
    selected: Option<usize>,
    labels: bool,
    viewport: Viewport,
    size: [u16; 2],
) -> impl ratatui::widgets::Widget + 'a {
    let [width, height] = size;
    let (x_bounds, y_bounds) = viewport.bounds();
    let planned_labels = plan_labels(graph, positions, visible, selected, labels, viewport, size);
    Canvas::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Topology · Braille "),
        )
        .marker(Marker::Braille)
        .x_bounds(x_bounds)
        .y_bounds(y_bounds)
        .paint(move |context| {
            let neighbors: BTreeSet<_> = selected
                .into_iter()
                .flat_map(|node| graph.neighbors(node))
                .collect();
            // Vẽ cạnh trước để node và nhãn luôn nằm phía trên đường nối.
            for edge in &graph.edges {
                let Some((a, b)) = edge.a.zip(edge.b) else {
                    continue;
                };
                if !visible.contains(&a)
                    || !visible.contains(&b)
                    || a >= positions.len()
                    || b >= positions.len()
                {
                    continue;
                }
                let highlighted = selected.is_some_and(|node| a == node || b == node);
                // Dịch vuông góc giúp các cáp song song không chồng khít.
                let offset = (edge.parallel_index as f64
                    - edge.parallel_count.saturating_sub(1) as f64 / 2.0)
                    * (x_bounds[1] - x_bounds[0])
                    / f64::from(width.max(1))
                    * 0.35;
                let dx = positions[b][0] - positions[a][0];
                let dy = positions[b][1] - positions[a][1];
                let length = dx.hypot(dy).max(1.0);
                let ox = -dy / length * offset;
                let oy = dx / length * offset;
                context.draw(&Line {
                    x1: positions[a][0] + ox,
                    y1: positions[a][1] + oy,
                    x2: positions[b][0] + ox,
                    y2: positions[b][1] + oy,
                    color: if highlighted {
                        Color::LightCyan
                    } else {
                        Color::Gray
                    },
                });
            }
            context.layer();
            let dot_x = (x_bounds[1] - x_bounds[0]) / (f64::from(width.max(1)) * 2.0);
            let dot_y = (y_bounds[1] - y_bounds[0]) / (f64::from(height.max(1)) * 4.0);
            // Node luôn là cụm chấm Braille; hub lớn hơn nhưng không đổi hình
            // theo loại, tránh trộn ký tự hình học với nét Braille của cạnh.
            for (index, node) in graph.nodes.iter().enumerate() {
                if !visible.contains(&index) || index >= positions.len() {
                    continue;
                }
                let related =
                    selected.is_none_or(|value| value == index || neighbors.contains(&index));
                let color = if selected == Some(index) {
                    Color::White
                } else {
                    type_color(&node.kind, related)
                };
                let radius = if graph.degree(index) >= 4 { 2 } else { 1 };
                let mut dots = Vec::new();
                for x in -radius..=radius {
                    for y in -radius..=radius {
                        if x * x + y * y <= radius * radius + 1 {
                            dots.push((
                                positions[index][0] + f64::from(x) * dot_x,
                                positions[index][1] + f64::from(y) * dot_y,
                            ));
                        }
                    }
                }
                context.draw(&Points {
                    coords: &dots,
                    color,
                });
                context.layer();
            }
            for label in &planned_labels {
                // Đây là phép nghịch đảo đúng của mapping label trong Canvas
                // ratatui với resolution `(width-1, height-1)`. Nếu dùng width
                // trực tiếp, hai hàng kế nhau có thể bị làm tròn thành một.
                let max_x = f64::from(width.saturating_sub(1));
                let max_y = f64::from(height.saturating_sub(1));
                let label_x = if f64::from(label.cell_x) < max_x {
                    f64::from(label.cell_x) + 0.25
                } else {
                    max_x
                };
                let label_y = if f64::from(label.cell_y) < max_y {
                    f64::from(label.cell_y) + 0.25
                } else {
                    max_y
                };
                let x = x_bounds[0]
                    + label_x / f64::from(width.saturating_sub(1).max(1))
                        * (x_bounds[1] - x_bounds[0]);
                let y = y_bounds[1]
                    - label_y / f64::from(height.saturating_sub(1).max(1))
                        * (y_bounds[1] - y_bounds[0]);
                let color = if selected == Some(label.node) {
                    Color::White
                } else {
                    type_color(&graph.nodes[label.node].kind, true)
                };
                context.print(
                    x,
                    y,
                    TextLine::styled(label.text.clone(), Style::default().fg(color)),
                );
            }
        })
}
