mod common;

use common::layered_info_48;
use pkt2ai::{
    graph::Graph,
    layout::{Point, fit_to_view, layered},
};
use std::collections::{BTreeSet, HashSet};

fn assert_fit(points: &[Point], width: u16, height: u16) {
    let visible: BTreeSet<_> = (0..points.len()).collect();
    let fit = fit_to_view(points, &visible, width, height);
    let (xb, yb) = fit.bounds(1.0);
    let source = fit.source_bounds;
    assert!(source[0] >= xb[0] && source[1] <= xb[1]);
    assert!(source[2] >= yb[0] && source[3] <= yb[1]);

    let ratio_x = (source[1] - source[0]) / (xb[1] - xb[0]);
    let ratio_y = (source[3] - source[2]) / (yb[1] - yb[0]);
    assert!((0.80..=0.91).contains(&ratio_x.max(ratio_y)));

    let source_center = [(source[0] + source[1]) / 2.0, (source[2] + source[3]) / 2.0];
    let error_cells_x =
        (source_center[0] - fit.center[0]).abs() / (xb[1] - xb[0]) * f64::from(width);
    let error_cells_y =
        (source_center[1] - fit.center[1]).abs() / (yb[1] - yb[0]) * f64::from(height);
    assert!(error_cells_x <= 1.0 && error_cells_y <= 1.0);
}

#[test]
fn fit_uses_braille_aspect_for_single_pair_long_and_tall_graphs() {
    assert_fit(&[[0.0, 0.0]], 80, 20);
    assert_fit(&[[-1.0, -1.0], [1.0, 1.0]], 80, 20);
    assert_fit(&[[-10_000.0, 0.0], [10_000.0, 0.0]], 80, 20);
    assert_fit(&[[0.0, -10_000.0], [0.0, 10_000.0]], 80, 20);
}

#[test]
fn layered_tree_has_unique_cells_and_adjacent_edge_levels() {
    let graph = Graph::new(&layered_info_48());
    let visible: BTreeSet<_> = (0..graph.nodes.len()).collect();
    let result = layered(&graph, &visible);
    let fit = fit_to_view(&result.positions, &visible, 72, 35);
    let (xb, yb) = fit.bounds(1.0);
    let mut occupied = HashSet::new();
    for point in &result.positions {
        let cell = (
            ((point[0] - xb[0]) / (xb[1] - xb[0]) * 71.0).round() as i32,
            ((yb[1] - point[1]) / (yb[1] - yb[0]) * 34.0).round() as i32,
        );
        assert!(
            occupied.insert(cell),
            "two nodes share terminal cell {cell:?}"
        );
    }
    for edge in &graph.edges {
        let (Some(a), Some(b)) = (edge.a, edge.b) else {
            continue;
        };
        assert_eq!(
            result.levels[a]
                .unwrap()
                .abs_diff(result.levels[b].unwrap()),
            1
        );
    }
}
