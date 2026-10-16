use crate::{
    graph::{Graph, node_style},
    layout::Point,
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

pub fn type_color(kind: &str, bright: bool) -> Color {
    let base = match kind.to_ascii_lowercase().replace([' ', '-'], "").as_str() {
        "router" => Color::Yellow,
        "switch" => Color::Blue,
        "multilayerswitch" => Color::Cyan,
        "pc" | "laptop" => Color::Green,
        "server" => Color::Red,
        "accesspoint" => Color::Magenta,
        "printer" => Color::White,
        _ => Color::Gray,
    };
    if bright { base } else { Color::DarkGray }
}

#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub center: Point,
    pub zoom: f64,
}
impl Default for Viewport {
    fn default() -> Self {
        Self {
            center: [0.0, 0.0],
            zoom: 1.0,
        }
    }
}
impl Viewport {
    pub fn bounds(self, aspect: f64) -> ([f64; 2], [f64; 2]) {
        let half_y = 80.0 / self.zoom.max(0.1);
        let half_x = half_y * aspect.max(0.5);
        (
            [self.center[0] - half_x, self.center[0] + half_x],
            [self.center[1] - half_y, self.center[1] + half_y],
        )
    }
}

pub fn widget<'a>(
    graph: &'a Graph,
    positions: &'a [Point],
    visible: &'a BTreeSet<usize>,
    selected: Option<usize>,
    labels: bool,
    viewport: Viewport,
    aspect: f64,
) -> impl ratatui::widgets::Widget + 'a {
    let (x_bounds, y_bounds) = viewport.bounds(aspect);
    let mut occupied = BTreeSet::new();
    let label_nodes: BTreeSet<usize> = if labels && viewport.zoom >= 0.65 {
        graph
            .nodes
            .iter()
            .enumerate()
            .filter(|(index, _)| visible.contains(index) && *index < positions.len())
            .filter_map(|(index, _)| {
                let x = ((positions[index][0] - x_bounds[0]) / (x_bounds[1] - x_bounds[0]) * 40.0)
                    .floor() as i32;
                let y = ((positions[index][1] - y_bounds[0]) / (y_bounds[1] - y_bounds[0]) * 20.0)
                    .floor() as i32;
                occupied.insert((x, y)).then_some(index)
            })
            .collect()
    } else {
        BTreeSet::new()
    };
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
                let highlighted = selected.is_none_or(|node| a == node || b == node);
                let offset = (edge.parallel_index as f64
                    - edge.parallel_count.saturating_sub(1) as f64 / 2.0)
                    * 1.5
                    / viewport.zoom;
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
                        Color::Gray
                    } else {
                        Color::DarkGray
                    },
                });
            }
            for (index, node) in graph.nodes.iter().enumerate() {
                if !visible.contains(&index) || index >= positions.len() {
                    continue;
                }
                let bright =
                    selected.is_none_or(|value| value == index || neighbors.contains(&index));
                let color = if selected == Some(index) {
                    Color::White
                } else {
                    type_color(&node.kind, bright)
                };
                context.draw(&Points {
                    coords: &[(positions[index][0], positions[index][1])],
                    color,
                });
                let symbol = node_style(&node.kind).symbol.to_string();
                context.print(
                    positions[index][0],
                    positions[index][1],
                    TextLine::styled(symbol, Style::default().fg(color)),
                );
                if label_nodes.contains(&index) {
                    context.print(
                        positions[index][0] + 2.0 / viewport.zoom,
                        positions[index][1],
                        TextLine::styled(node.name.clone(), Style::default().fg(color)),
                    );
                }
            }
        })
}
