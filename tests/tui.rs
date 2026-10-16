#![cfg(feature = "tui")]

mod common;

use common::SAMPLE_XML;
use crossterm::event::{KeyCode, KeyEvent};
use pkt2ai::{extract, tui::TopologyApp};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};
use std::fs;

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
        assert!(text.contains("Chi tiet"));
        assert!(text.contains("PC 1"));
        assert!(text.contains("MASKED"));
        if width == 80 {
            fs::write("/tmp/pkt2ai-tui-80x24.txt", &text).unwrap();
            assert_eq!(text, include_str!("snapshots/tui_80x24.txt"));
        }
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
    assert!(buffer_text(terminal.backend().buffer()).contains("Switch - tang1"));
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
