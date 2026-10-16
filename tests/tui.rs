#![cfg(feature = "tui")]

mod common;

use common::{SAMPLE_XML, layered_info_48};
use crossterm::event::{KeyCode, KeyEvent};
use pkt2ai::{
    extract,
    tui::{
        TopologyApp,
        app::LayoutMode,
        canvas::{plan_labels, type_color},
    },
};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, style::Color};
use std::{collections::BTreeSet, fs, path::Path};

fn buffer_text(buffer: &Buffer) -> String {
    let width = usize::from(buffer.area().width);
    buffer
        .content()
        .chunks(width)
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn app() -> TopologyApp {
    TopologyApp::from_info(extract(SAMPLE_XML.as_bytes(), "fixture.xml", true, false).unwrap())
}

#[test]
fn renders_at_small_and_large_terminal_sizes() {
    for (width, height) in [(80, 24), (160, 50)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let mut app = app();
        terminal.draw(|frame| app.render(frame)).unwrap();
        let text = buffer_text(terminal.backend().buffer());
        assert!(text.contains("Topology · Braille"));
        assert!(text.contains("Chi tiết"));
        assert!(text.contains("PC 1"));
        assert!(text.contains("MASKED"));
        assert!(text.contains("100% (fit)"));
    }
    let mut terminal = Terminal::new(TestBackend::new(20, 5)).unwrap();
    let mut app = app();
    terminal.draw(|frame| app.render(frame)).unwrap();
    assert!(buffer_text(terminal.backend().buffer()).contains("Terminal qua nho"));
}

#[test]
fn keyboard_changes_selection_panel_and_opens_dialogs() {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let mut app = app();
    assert_eq!(app.selected, Some(0));
    app.handle_key(KeyEvent::from(KeyCode::Tab));
    assert_eq!(app.selected, Some(1));
    terminal.draw(|frame| app.render(frame)).unwrap();
    let panel = buffer_text(terminal.backend().buffer());
    assert!(panel.contains("Switch - tang1"));
    assert!(panel.contains("Fa0/1"));
    assert!(panel.contains("→  PC 1"));
    assert!(panel.contains("cổng Fa0"));
    app.handle_key(KeyEvent::from(KeyCode::Char('p')));
    terminal.draw(|frame| app.render(frame)).unwrap();
    assert!(!buffer_text(terminal.backend().buffer()).contains("Chi tiết"));
    app.handle_key(KeyEvent::from(KeyCode::Char('p')));
    app.handle_key(KeyEvent::from(KeyCode::Char('e')));
    terminal.draw(|frame| app.render(frame)).unwrap();
    assert!(buffer_text(terminal.backend().buffer()).contains("Xuat"));
    app.handle_key(KeyEvent::from(KeyCode::Esc));
    app.handle_key(KeyEvent::from(KeyCode::Char('/')));
    for character in "r&d".chars() {
        app.handle_key(KeyEvent::from(KeyCode::Char(character)));
    }
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    assert_eq!(app.selected, Some(2));
    app.handle_key(KeyEvent::from(KeyCode::Char('?')));
    terminal.draw(|frame| app.render(frame)).unwrap();
    assert!(buffer_text(terminal.backend().buffer()).contains("Tro giup"));
}

#[test]
fn graph_has_visible_edges_distinct_type_colors_and_non_overlapping_labels() {
    let mut app = TopologyApp::from_info(layered_info_48());
    app.set_layout_for_test(LayoutMode::Layered);
    app.selected = None;
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    let area = app.canvas_area_for_test();
    let colors: BTreeSet<_> = buffer
        .content()
        .iter()
        .map(|cell| format!("{:?}", cell.fg))
        .collect();
    let expected: BTreeSet<_> = app
        .graph
        .nodes
        .iter()
        .map(|node| format!("{:?}", type_color(&node.kind, true)))
        .collect();
    assert!(expected.len() >= 4);
    assert!(expected.iter().all(|color| colors.contains(color)));
    assert!(buffer.content().iter().any(|cell| cell.fg == Color::Gray));

    app.selected = Some(0);
    terminal.draw(|frame| app.render(frame)).unwrap();
    let visible = app.visible_for_test();
    let labels = plan_labels(
        &app.graph,
        &app.positions,
        &visible,
        app.selected,
        true,
        app.viewport_for_test(),
        [area.width.saturating_sub(2), area.height.saturating_sub(2)],
    );
    assert!(labels.iter().any(|label| label.node == 0));
    assert!(
        app.graph
            .neighbors(0)
            .all(|neighbor| labels.iter().any(|label| label.node == neighbor))
    );
    for (index, left) in labels.iter().enumerate() {
        for right in &labels[index + 1..] {
            assert!(
                left.cell_y != right.cell_y
                    || left.cell_x + left.cell_width <= right.cell_x
                    || right.cell_x + right.cell_width <= left.cell_x
            );
        }
    }
    let buffer = terminal.backend().buffer();
    for label in &labels {
        let rendered: String = (0..label.cell_width)
            .map(|offset| {
                buffer[(
                    area.x + 1 + u16::try_from(label.cell_x + offset).unwrap(),
                    area.y + 1 + u16::try_from(label.cell_y).unwrap(),
                )]
                    .symbol()
            })
            .collect();
        assert_eq!(rendered, label.text, "label was overwritten in buffer");
    }
    assert!(buffer_text(buffer).contains("Router-HQ"));
}

#[test]
fn renders_nine_readability_snapshots_and_prints_metrics() {
    let output = Path::new("target/tui-snapshots");
    fs::create_dir_all(output).unwrap();
    for (width, height) in [(120, 40), (160, 50), (80, 24)] {
        for (mode, mode_name) in [
            (LayoutMode::PacketTracer, "pt"),
            (LayoutMode::Layered, "layered"),
            (LayoutMode::Force, "force"),
        ] {
            let mut app = TopologyApp::from_info(layered_info_48());
            app.set_layout_for_test(mode);
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| app.render(frame)).unwrap();
            let text = buffer_text(terminal.backend().buffer());
            fs::write(
                output.join(format!("{width}x{height}-{mode_name}.txt")),
                text,
            )
            .unwrap();

            let viewport = app.viewport_for_test();
            let (xb, yb) = viewport.bounds();
            let source = viewport.fit.source_bounds;
            let ratio_x = (source[1] - source[0]) / (xb[1] - xb[0]);
            let ratio_y = (source[3] - source[2]) / (yb[1] - yb[0]);
            assert!((0.80..=0.91).contains(&ratio_x.max(ratio_y)));
            let area = app.canvas_area_for_test();
            let labels = plan_labels(
                &app.graph,
                &app.positions,
                &app.visible_for_test(),
                app.selected,
                true,
                viewport,
                [area.width.saturating_sub(2), area.height.saturating_sub(2)],
            );
            let colors: BTreeSet<_> = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .filter(|cell| cell.fg != Color::Reset)
                .map(|cell| format!("{:?}", cell.fg))
                .collect();
            println!(
                "TUI_METRIC {width}x{height} {mode_name} bbox={ratio_x:.3}x{ratio_y:.3} labels={} colors={}",
                labels.len(),
                colors.len()
            );
        }
    }
}
