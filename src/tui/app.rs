use super::{
    canvas::{Viewport, widget as graph_widget},
    export_dialog::{ExportDialog, TuiExportFormat},
    picker::{FilePicker, remember_directory, remembered_directory},
};
use crate::{
    Counts, Info, extract,
    graph::Graph,
    layout::{Point, force_directed, packet_tracer},
    to_markdown, xml_to_pkt,
};
use crossterm::{
    cursor::{Hide, Show},
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        MouseButton, MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, stdout},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

const LAYOUT_SEED: u64 = 0x504b_5432_4149;

#[derive(Debug)]
enum Mode {
    Normal,
    Help,
    Search(String),
    Filter { kinds: Vec<String>, selected: usize },
    Picker(FilePicker),
    Export(ExportDialog),
    Confirm(ExportDialog),
    Message(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayoutMode {
    Force,
    PacketTracer,
}

pub struct TopologyApp {
    pub info: Info,
    pub graph: Graph,
    pub positions: Vec<Point>,
    pub selected: Option<usize>,
    pub should_quit: bool,
    xml: Option<Vec<u8>>,
    path: Option<PathBuf>,
    redact: bool,
    labels: bool,
    show_isolated: bool,
    kind_filter: BTreeSet<String>,
    local_depth: Option<usize>,
    viewport: Viewport,
    layout_mode: LayoutMode,
    layout_rx: Option<Receiver<Vec<Point>>>,
    mode: Mode,
    link_cursor: usize,
    canvas_area: Rect,
    drag_at: Option<(u16, u16)>,
    status: String,
}

impl TopologyApp {
    pub fn from_info(info: Info) -> Self {
        let graph = Graph::new(&info);
        let positions = force_directed(&graph, LAYOUT_SEED, 100);
        let selected = (!graph.nodes.is_empty()).then_some(0);
        Self {
            info,
            graph,
            positions,
            selected,
            should_quit: false,
            xml: None,
            path: None,
            redact: true,
            labels: true,
            show_isolated: false,
            kind_filter: BTreeSet::new(),
            local_depth: None,
            viewport: Viewport::default(),
            layout_mode: LayoutMode::Force,
            layout_rx: None,
            mode: Mode::Normal,
            link_cursor: 0,
            canvas_area: Rect::default(),
            drag_at: None,
            status: String::new(),
        }
    }

    pub fn open(path: &Path, redact: bool) -> Result<Self, String> {
        let data = fs::read(path).map_err(|error| error.to_string())?;
        let first = data
            .iter()
            .find(|byte| !byte.is_ascii_whitespace())
            .copied();
        let (info, xml) = if first == Some(b'{') {
            (
                serde_json::from_slice(&data)
                    .map_err(|error| format!("JSON khong hop le: {error}"))?,
                None,
            )
        } else {
            let xml = if crate::ptformat::looks_like_xml(&data) {
                data
            } else {
                crate::pkt_to_xml(&data).map_err(|error| error.to_string())?
            };
            let source = path
                .file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy();
            let info = extract(&xml, &source, redact, false).map_err(|error| error.to_string())?;
            (info, Some(xml))
        };
        let mut app = Self::from_info(info);
        app.xml = xml;
        app.path = Some(path.to_path_buf());
        app.redact = redact;
        if let Some(directory) = path.parent() {
            remember_directory(directory);
        }
        app.start_layout();
        Ok(app)
    }

    pub fn start_picker() -> Self {
        let info = Info {
            source: String::new(),
            pt_version: String::new(),
            counts: Counts {
                devices: 0,
                links: 0,
                by_type: BTreeMap::new(),
            },
            devices: Vec::new(),
            links: Vec::new(),
            warnings: Vec::new(),
        };
        let mut app = Self::from_info(info);
        app.mode = Mode::Picker(FilePicker::new(remembered_directory()));
        app
    }

    fn start_layout(&mut self) {
        let graph = self.graph.clone();
        let seed = LAYOUT_SEED;
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(force_directed(
                &graph,
                seed,
                if graph.nodes.len() > 300 { 70 } else { 120 },
            ));
        });
        self.layout_rx = Some(rx);
        self.status = "dang bo tri force tren worker…".to_owned();
    }

    fn poll_layout(&mut self) {
        if let Some(receiver) = &self.layout_rx
            && let Ok(positions) = receiver.try_recv()
        {
            self.positions = positions;
            self.fit_view();
            self.layout_rx = None;
            self.status = "bo tri force da san sang".to_owned();
        }
    }

    fn visible(&self) -> BTreeSet<usize> {
        self.graph.visible_nodes(
            &self.kind_filter,
            self.show_isolated,
            self.selected.zip(self.local_depth),
        )
    }

    fn ensure_selection(&mut self) {
        let visible = self.visible();
        if self
            .selected
            .is_none_or(|selected| !visible.contains(&selected))
        {
            self.selected = visible.first().copied();
        }
    }

    pub fn render(&mut self, frame: &mut Frame<'_>) {
        self.poll_layout();
        self.ensure_selection();
        let area = frame.area();
        if area.width < 40 || area.height < 10 {
            frame.render_widget(
                Paragraph::new("pkt2ai TUI\nTerminal qua nho; can it nhat 40x10.\nq: thoat")
                    .block(Block::default().borders(Borders::ALL)),
                area,
            );
            return;
        }
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(5), Constraint::Length(2)])
            .split(area);
        let panel_width = if area.width < 100 { 30 } else { 40 };
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(20), Constraint::Length(panel_width)])
            .split(rows[0]);
        self.canvas_area = columns[0];
        let visible = self.visible();
        let aspect = f64::from(columns[0].width.saturating_sub(2))
            / f64::from(columns[0].height.saturating_sub(2).max(1));
        frame.render_widget(
            graph_widget(
                &self.graph,
                &self.positions,
                &visible,
                self.selected,
                self.labels,
                self.viewport,
                aspect,
            ),
            columns[0],
        );
        frame.render_widget(self.detail_widget(), columns[1]);
        frame.render_widget(self.status_widget(visible.len()), rows[1]);
        self.render_overlay(frame, area);
    }

    fn detail_widget(&self) -> Paragraph<'_> {
        let text = self
            .selected
            .and_then(|index| self.graph.nodes.get(index))
            .map_or_else(
                || "Chua chon node".to_owned(),
                |device| {
                    let mut text = format!("{}\n{} {}\n\n", device.name, device.kind, device.model);
                    for port in &device.ports {
                        if let Some(ip) = &port.ip {
                            text.push_str(&format!("{}: {ip}\n", port.kind));
                        }
                    }
                    for interface in &device.interfaces {
                        text.push_str(&format!(
                            "{}: {}\n",
                            interface.name,
                            interface.ip.as_deref().unwrap_or("-")
                        ));
                    }
                    text.push_str("\nLien ket (j/k, Enter):\n");
                    for (index, link) in
                        crate::links_of(&self.info, &device.name).iter().enumerate()
                    {
                        let (peer, own, remote) = if link.a == device.name {
                            (&link.b, &link.a_port, &link.b_port)
                        } else {
                            (&link.a, &link.b_port, &link.a_port)
                        };
                        text.push_str(&format!(
                            "{} {} ↔ {}:{}\n",
                            if index == self.link_cursor { ">" } else { " " },
                            own,
                            peer,
                            remote
                        ));
                    }
                    if !device.config_other.is_empty() {
                        text.push_str("\nConfig:\n");
                        for line in &device.config_other {
                            text.push_str(line);
                            text.push('\n');
                        }
                    }
                    text
                },
            );
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title(" Chi tiet "))
    }

    fn status_widget(&self, visible: usize) -> Paragraph<'_> {
        let masked = if self.redact {
            "MASKED"
        } else {
            "SECRETS VISIBLE"
        };
        let layout = if self.layout_mode == LayoutMode::Force {
            "force"
        } else {
            "Packet Tracer"
        };
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {masked} "),
                Style::default()
                    .fg(if self.redact {
                        Color::Green
                    } else {
                        Color::Red
                    })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(
                " {visible}/{} node · {layout} · zoom {:.1} · local {} · {}   ?:help",
                self.graph.nodes.len(),
                self.viewport.zoom,
                self.local_depth
                    .map_or("off".to_owned(), |depth| depth.to_string()),
                self.status
            )),
        ]))
        .style(Style::default().bg(Color::Black))
    }

    fn render_overlay(&self, frame: &mut Frame<'_>, area: Rect) {
        let popup = centered(
            area,
            72.min(area.width.saturating_sub(4)),
            20.min(area.height.saturating_sub(2)),
        );
        match &self.mode {
            Mode::Normal => {}
            Mode::Help => {
                frame.render_widget(Clear, popup);
                let help = vec![
                    Line::raw("? help  q quit  o open  e export  / search  f filter"),
                    Line::raw("l labels  r relayout  g force/original  +/- zoom  arrows pan"),
                    Line::raw("[ ] local depth  s secrets  i isolated  Tab select"),
                    Line::raw("j/k choose link, Enter jump  mouse drag/scroll/click"),
                    Line::raw(""),
                    Line::from(vec![
                        Span::raw("Mau: "),
                        Span::styled("◆ Router  ", Style::default().fg(Color::Yellow)),
                        Span::styled("■ Switch  ", Style::default().fg(Color::Blue)),
                        Span::styled("⬢ L3  ", Style::default().fg(Color::Cyan)),
                        Span::styled("● PC/Laptop  ", Style::default().fg(Color::Green)),
                        Span::styled("▣ Server  ", Style::default().fg(Color::Red)),
                        Span::styled("▲ AP  ", Style::default().fg(Color::Magenta)),
                        Span::styled("▤ Printer", Style::default().fg(Color::White)),
                    ]),
                    Line::raw(""),
                    Line::raw("Esc/?: dong"),
                ];
                frame.render_widget(
                    Paragraph::new(help)
                        .wrap(Wrap { trim: false })
                        .block(Block::default().borders(Borders::ALL).title(" Tro giup ")),
                    popup,
                );
            }
            Mode::Search(query) => {
                let search = centered(area, 60.min(area.width.saturating_sub(4)), 3);
                frame.render_widget(Clear, search);
                frame.render_widget(
                    Paragraph::new(format!("/{query}")).block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Tim ten · Enter "),
                    ),
                    search,
                );
            }
            Mode::Filter { kinds, selected } => {
                frame.render_widget(Clear, popup);
                let items: Vec<_> = kinds
                    .iter()
                    .enumerate()
                    .map(|(index, kind)| {
                        ListItem::new(format!(
                            "{} {}",
                            if self.kind_filter.contains(kind) {
                                "[x]"
                            } else {
                                "[ ]"
                            },
                            kind
                        ))
                        .style(if index == *selected {
                            Style::default().bg(Color::Blue)
                        } else {
                            Style::default()
                        })
                    })
                    .collect();
                frame.render_widget(
                    List::new(items).block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Loc loai · Space · Enter "),
                    ),
                    popup,
                );
            }
            Mode::Picker(picker) => {
                frame.render_widget(Clear, popup);
                let items: Vec<_> = picker
                    .entries
                    .iter()
                    .enumerate()
                    .map(|(index, path)| {
                        ListItem::new(format!(
                            "{}{}",
                            path.file_name().unwrap_or_default().to_string_lossy(),
                            if path.is_dir() { "/" } else { "" }
                        ))
                        .style(if index == picker.selected {
                            Style::default().bg(Color::Blue)
                        } else {
                            Style::default()
                        })
                    })
                    .collect();
                frame.render_widget(
                    List::new(items).block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(format!(" Mo file · {} ", picker.directory.display())),
                    ),
                    popup,
                );
            }
            Mode::Export(dialog) | Mode::Confirm(dialog) => {
                frame.render_widget(Clear, popup);
                let mut lines = Vec::new();
                for (index, format) in TuiExportFormat::ALL.iter().enumerate() {
                    let available = !format.needs_xml() || self.xml.is_some();
                    lines.push(Line::styled(
                        format!(
                            "{} {:<18} {}",
                            if index == dialog.selected { ">" } else { " " },
                            format.label(),
                            if available { "" } else { "(khong co XML)" }
                        ),
                        Style::default().fg(if available {
                            Color::White
                        } else {
                            Color::DarkGray
                        }),
                    ));
                }
                lines.push(Line::raw(""));
                lines.push(Line::raw(format!(
                    "Path{}: {}",
                    if dialog.editing_path { " [typing]" } else { "" },
                    dialog.path
                )));
                lines.push(Line::raw(format!(
                    "Scope: {} (Tab)",
                    if dialog.visible_only {
                        "chi phan dang hien"
                    } else {
                        "toan bo"
                    }
                )));
                if matches!(self.mode, Mode::Confirm(_)) {
                    lines.push(Line::styled(
                        "Da ton tai. y: ghi de · n: huy",
                        Style::default().fg(Color::Yellow),
                    ));
                } else {
                    lines.push(Line::raw("↑↓ format · p sua path · Enter xuat · Esc"));
                }
                frame.render_widget(
                    Paragraph::new(lines)
                        .block(Block::default().borders(Borders::ALL).title(" Xuat ")),
                    popup,
                );
            }
            Mode::Message(message) => {
                let message_area = centered(area, 70.min(area.width.saturating_sub(4)), 7);
                frame.render_widget(Clear, message_area);
                frame.render_widget(
                    Paragraph::new(message.clone())
                        .alignment(Alignment::Center)
                        .wrap(Wrap { trim: true })
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title(" pkt2ai · Enter/Esc "),
                        ),
                    message_area,
                );
            }
        }
    }

    pub fn handle_key(&mut self, event: KeyEvent) {
        if event.kind == KeyEventKind::Release {
            return;
        }
        let mode = std::mem::replace(&mut self.mode, Mode::Normal);
        self.mode = match mode {
            Mode::Normal => {
                self.handle_normal(event.code);
                std::mem::replace(&mut self.mode, Mode::Normal)
            }
            Mode::Help => {
                if matches!(event.code, KeyCode::Esc | KeyCode::Char('?')) {
                    Mode::Normal
                } else {
                    Mode::Help
                }
            }
            Mode::Message(message) => {
                if matches!(event.code, KeyCode::Esc | KeyCode::Enter) {
                    Mode::Normal
                } else {
                    Mode::Message(message)
                }
            }
            Mode::Search(mut query) => match event.code {
                KeyCode::Esc => Mode::Normal,
                KeyCode::Enter => {
                    let lowered = query.to_lowercase();
                    if let Some(index) = self
                        .graph
                        .nodes
                        .iter()
                        .position(|node| node.name.to_lowercase().contains(&lowered))
                    {
                        self.selected = Some(index);
                        self.status = format!("da chon {}", self.graph.nodes[index].name);
                    } else {
                        self.status = format!("khong tim thay '{query}'");
                    }
                    Mode::Normal
                }
                KeyCode::Backspace => {
                    query.pop();
                    Mode::Search(query)
                }
                KeyCode::Char(character) => {
                    query.push(character);
                    Mode::Search(query)
                }
                _ => Mode::Search(query),
            },
            Mode::Filter {
                kinds,
                mut selected,
            } => match event.code {
                KeyCode::Esc | KeyCode::Enter => Mode::Normal,
                KeyCode::Up => {
                    selected = selected.saturating_sub(1);
                    Mode::Filter { kinds, selected }
                }
                KeyCode::Down => {
                    selected = (selected + 1).min(kinds.len().saturating_sub(1));
                    Mode::Filter { kinds, selected }
                }
                KeyCode::Char(' ') => {
                    if let Some(kind) = kinds.get(selected) {
                        if self.kind_filter.contains(kind) {
                            if self.kind_filter.len() > 1 {
                                self.kind_filter.remove(kind);
                            }
                        } else {
                            self.kind_filter.insert(kind.clone());
                        }
                    }
                    Mode::Filter { kinds, selected }
                }
                _ => Mode::Filter { kinds, selected },
            },
            Mode::Picker(mut picker) => match event.code {
                KeyCode::Esc => Mode::Normal,
                KeyCode::Up => {
                    picker.move_by(-1);
                    Mode::Picker(picker)
                }
                KeyCode::Down => {
                    picker.move_by(1);
                    Mode::Picker(picker)
                }
                KeyCode::Backspace | KeyCode::Left => {
                    picker.parent();
                    Mode::Picker(picker)
                }
                KeyCode::Enter => {
                    if let Some(path) = picker.choose() {
                        match Self::open(&path, true) {
                            Ok(app) => {
                                *self = app;
                                Mode::Normal
                            }
                            Err(error) => Mode::Message(error),
                        }
                    } else {
                        Mode::Picker(picker)
                    }
                }
                _ => Mode::Picker(picker),
            },
            Mode::Export(mut dialog) => match event.code {
                KeyCode::Esc => Mode::Normal,
                KeyCode::Up if !dialog.editing_path => {
                    dialog.move_by(-1);
                    Mode::Export(dialog)
                }
                KeyCode::Down if !dialog.editing_path => {
                    dialog.move_by(1);
                    Mode::Export(dialog)
                }
                KeyCode::Tab => {
                    dialog.visible_only = !dialog.visible_only;
                    Mode::Export(dialog)
                }
                KeyCode::Char('p') if !dialog.editing_path => {
                    dialog.editing_path = true;
                    Mode::Export(dialog)
                }
                KeyCode::Backspace if dialog.editing_path => {
                    dialog.path.pop();
                    Mode::Export(dialog)
                }
                KeyCode::Char(character) if dialog.editing_path => {
                    dialog.path.push(character);
                    Mode::Export(dialog)
                }
                KeyCode::Enter if dialog.editing_path => {
                    dialog.editing_path = false;
                    Mode::Export(dialog)
                }
                KeyCode::Enter => match self.write_export(&dialog, false) {
                    Ok(message) => Mode::Message(message),
                    Err(error) if error.starts_with("EXISTS:") => Mode::Confirm(dialog),
                    Err(error) => Mode::Message(error),
                },
                _ => Mode::Export(dialog),
            },
            Mode::Confirm(dialog) => match event.code {
                KeyCode::Char('y') => match self.write_export(&dialog, true) {
                    Ok(message) => Mode::Message(message),
                    Err(error) => Mode::Message(error),
                },
                KeyCode::Char('n') | KeyCode::Esc => Mode::Export(dialog),
                _ => Mode::Confirm(dialog),
            },
        };
    }

    fn handle_normal(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.mode = Mode::Help,
            KeyCode::Char('o') => {
                self.mode = Mode::Picker(FilePicker::new(
                    self.path
                        .as_ref()
                        .and_then(|path| path.parent())
                        .unwrap_or(Path::new(".")),
                ))
            }
            KeyCode::Char('e') => {
                self.mode =
                    Mode::Export(ExportDialog::new(default_export_path(self.path.as_deref())))
            }
            KeyCode::Char('/') => self.mode = Mode::Search(String::new()),
            KeyCode::Char('f') => {
                let kinds: BTreeSet<_> = self
                    .graph
                    .nodes
                    .iter()
                    .map(|node| node.kind.clone())
                    .collect();
                if self.kind_filter.is_empty() {
                    self.kind_filter = kinds.clone();
                }
                self.mode = Mode::Filter {
                    kinds: kinds.into_iter().collect(),
                    selected: 0,
                };
            }
            KeyCode::Char('l') => self.labels = !self.labels,
            KeyCode::Char('i') => self.show_isolated = !self.show_isolated,
            KeyCode::Char('r') => {
                self.layout_mode = LayoutMode::Force;
                self.start_layout();
            }
            KeyCode::Char('g') => {
                if self.layout_mode == LayoutMode::Force {
                    self.layout_mode = LayoutMode::PacketTracer;
                    self.positions = packet_tracer(&self.graph, LAYOUT_SEED);
                    self.fit_view();
                    self.layout_rx = None;
                } else {
                    self.layout_mode = LayoutMode::Force;
                    self.start_layout();
                }
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                self.viewport.zoom = (self.viewport.zoom * 1.2).min(20.0)
            }
            KeyCode::Char('-') => self.viewport.zoom = (self.viewport.zoom / 1.2).max(0.1),
            KeyCode::Left => self.viewport.center[0] -= 10.0 / self.viewport.zoom,
            KeyCode::Right => self.viewport.center[0] += 10.0 / self.viewport.zoom,
            KeyCode::Up => self.viewport.center[1] += 10.0 / self.viewport.zoom,
            KeyCode::Down => self.viewport.center[1] -= 10.0 / self.viewport.zoom,
            KeyCode::Char(']') => self.local_depth = Some(self.local_depth.unwrap_or(0) + 1),
            KeyCode::Char('[') => {
                self.local_depth = self.local_depth.and_then(|depth| depth.checked_sub(1))
            }
            KeyCode::Char('s') => self.toggle_secrets(),
            KeyCode::Tab => self.select_next(),
            KeyCode::Char('j') => self.link_cursor = self.link_cursor.saturating_add(1),
            KeyCode::Char('k') => self.link_cursor = self.link_cursor.saturating_sub(1),
            KeyCode::Enter => self.jump_link(),
            _ => {}
        }
    }

    fn select_next(&mut self) {
        let visible: Vec<_> = self.visible().into_iter().collect();
        if visible.is_empty() {
            self.selected = None;
            return;
        }
        self.selected = Some(
            match self
                .selected
                .and_then(|current| visible.iter().position(|value| *value == current))
            {
                Some(index) => visible[(index + 1) % visible.len()],
                None => visible[0],
            },
        );
        self.link_cursor = 0;
    }

    fn fit_view(&mut self) {
        let [min_x, max_x, min_y, max_y] = crate::layout::bounds(&self.positions);
        self.viewport.center = [(min_x + max_x) / 2.0, (min_y + max_y) / 2.0];
        self.viewport.zoom = (160.0 / (max_x - min_x).max(max_y - min_y).max(1.0)).clamp(0.1, 5.0);
    }

    fn jump_link(&mut self) {
        let Some(node) = self.selected.and_then(|index| self.graph.nodes.get(index)) else {
            return;
        };
        let links = crate::links_of(&self.info, &node.name);
        if links.is_empty() {
            return;
        }
        let link = links[self.link_cursor.min(links.len() - 1)];
        let peer = if link.a == node.name {
            &link.b
        } else {
            &link.a
        };
        if let Some(index) = self.graph.node_index(peer) {
            self.selected = Some(index);
            self.link_cursor = 0;
        }
    }

    fn toggle_secrets(&mut self) {
        let Some(xml) = &self.xml else {
            self.status = "nguon JSON khong co config goc de doi che secret".to_owned();
            return;
        };
        self.redact = !self.redact;
        match extract(xml, &self.info.source, self.redact, false) {
            Ok(info) => {
                self.info = info;
                self.graph = Graph::new(&self.info);
                self.status = if self.redact {
                    "da che secret"
                } else {
                    "CANH BAO: dang hien secret"
                }
                .to_owned();
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    pub fn handle_mouse(&mut self, event: MouseEvent) {
        match event.kind {
            MouseEventKind::ScrollUp => self.viewport.zoom = (self.viewport.zoom * 1.15).min(20.0),
            MouseEventKind::ScrollDown => self.viewport.zoom = (self.viewport.zoom / 1.15).max(0.1),
            MouseEventKind::Down(MouseButton::Left) => {
                self.drag_at = Some((event.column, event.row));
                self.select_at(event.column, event.row);
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some((column, row)) = self.drag_at.replace((event.column, event.row)) {
                    self.viewport.center[0] -=
                        f64::from(event.column as i32 - column as i32) * 3.0 / self.viewport.zoom;
                    self.viewport.center[1] +=
                        f64::from(event.row as i32 - row as i32) * 6.0 / self.viewport.zoom;
                }
            }
            MouseEventKind::Up(MouseButton::Left) => self.drag_at = None,
            _ => {}
        }
    }

    fn select_at(&mut self, column: u16, row: u16) {
        let area = self.canvas_area;
        if column <= area.x || row <= area.y || column >= area.right() || row >= area.bottom() {
            return;
        }
        let aspect = f64::from(area.width.saturating_sub(2))
            / f64::from(area.height.saturating_sub(2).max(1));
        let (xb, yb) = self.viewport.bounds(aspect);
        let x = xb[0]
            + f64::from(column - area.x - 1) / f64::from(area.width.saturating_sub(2).max(1))
                * (xb[1] - xb[0]);
        let y = yb[1]
            - f64::from(row - area.y - 1) / f64::from(area.height.saturating_sub(2).max(1))
                * (yb[1] - yb[0]);
        self.selected = self
            .visible()
            .into_iter()
            .filter_map(|index| {
                self.positions
                    .get(index)
                    .map(|point| (index, (point[0] - x).hypot(point[1] - y)))
            })
            .min_by(|left, right| left.1.total_cmp(&right.1))
            .filter(|(_, distance)| *distance < 15.0 / self.viewport.zoom)
            .map(|(index, _)| index);
    }

    fn scoped_info(&self, visible_only: bool) -> Info {
        if !visible_only {
            return self.info.clone();
        }
        let visible = self.visible();
        let names: BTreeSet<_> = visible
            .iter()
            .map(|index| self.graph.nodes[*index].name.clone())
            .collect();
        let devices: Vec<_> = visible
            .iter()
            .map(|index| self.graph.nodes[*index].clone())
            .collect();
        let links: Vec<_> = self
            .info
            .links
            .iter()
            .filter(|link| names.contains(&link.a) && names.contains(&link.b))
            .cloned()
            .collect();
        let mut by_type = BTreeMap::new();
        for device in &devices {
            *by_type.entry(device.kind.clone()).or_insert(0) += 1;
        }
        Info {
            source: self.info.source.clone(),
            pt_version: self.info.pt_version.clone(),
            counts: Counts {
                devices: devices.len(),
                links: links.len(),
                by_type,
            },
            devices,
            links,
            warnings: self.info.warnings.clone(),
        }
    }

    fn write_export(&self, dialog: &ExportDialog, force: bool) -> Result<String, String> {
        let format = dialog.format();
        if format.needs_xml() && self.xml.is_none() {
            return Err("Dinh dang nay khong kha dung: nguon JSON khong co XML".to_owned());
        }
        let path = PathBuf::from(&dialog.path);
        let info = self.scoped_info(dialog.visible_only);
        let graph = Graph::new(&info);
        if format == TuiExportFormat::Obsidian {
            let notes = crate::export::obsidian(&info);
            if path.exists() && !path.is_dir() {
                return Err("Dich khong phai thu muc".to_owned());
            }
            if !force && notes.iter().any(|(name, _)| path.join(name).exists()) {
                return Err("EXISTS:vault co file trung".to_owned());
            }
            fs::create_dir_all(&path).map_err(|error| error.to_string())?;
            for (name, content) in notes {
                fs::write(path.join(name), content).map_err(|error| error.to_string())?;
            }
            return Ok(format!("Da xuat Obsidian vault: {}", path.display()));
        }
        if path.exists() && !force {
            return Err(format!("EXISTS:{}", path.display()));
        }
        let bytes = match format {
            TuiExportFormat::Json => format!(
                "{}\n",
                serde_json::to_string_pretty(&info).map_err(|error| error.to_string())?
            )
            .into_bytes(),
            TuiExportFormat::Markdown => to_markdown(&info).into_bytes(),
            TuiExportFormat::Xml => self.xml.clone().expect("checked XML"),
            TuiExportFormat::Pkt => xml_to_pkt(self.xml.as_ref().expect("checked XML"))
                .map_err(|error| error.to_string())?,
            TuiExportFormat::Dot => crate::export::dot(&graph).into_bytes(),
            TuiExportFormat::Mermaid => crate::export::mermaid(&graph).into_bytes(),
            TuiExportFormat::Graphml => crate::export::graphml(&graph).into_bytes(),
            TuiExportFormat::Svg => {
                let positions: Vec<_> = graph
                    .nodes
                    .iter()
                    .map(|node| {
                        self.graph
                            .node_index(&node.name)
                            .and_then(|index| self.positions.get(index))
                            .copied()
                            .unwrap_or([0.0, 0.0])
                    })
                    .collect();
                crate::export::svg(&graph, &positions, 1000, 700).into_bytes()
            }
            TuiExportFormat::Obsidian => unreachable!(),
        };
        fs::write(&path, bytes).map_err(|error| error.to_string())?;
        Ok(format!("Da xuat {}", path.display()))
    }
}

fn default_export_path(source: Option<&Path>) -> String {
    source.and_then(Path::file_stem).map_or_else(
        || "topology.json".to_owned(),
        |stem| format!("{}.json", stem.to_string_lossy()),
    )
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width.min(area.width),
        height.min(area.height),
    )
}

struct TerminalRestore;
impl Drop for TerminalRestore {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), Show, DisableMouseCapture, LeaveAlternateScreen);
    }
}

pub fn run(path: Option<PathBuf>) -> Result<(), String> {
    let mut app = path
        .as_deref()
        .map(|path| TopologyApp::open(path, true))
        .transpose()?
        .unwrap_or_else(TopologyApp::start_picker);
    let restore = TerminalRestore;
    enable_raw_mode().map_err(|error| error.to_string())?;
    execute!(stdout(), EnterAlternateScreen, EnableMouseCapture, Hide)
        .map_err(|error| error.to_string())?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend).map_err(|error| error.to_string())?;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> io::Result<()> {
        while !app.should_quit {
            terminal.draw(|frame| app.render(frame))?;
            if event::poll(Duration::from_millis(50))? {
                match event::read()? {
                    Event::Key(key) => app.handle_key(key),
                    Event::Mouse(mouse) => app.handle_mouse(mouse),
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }
        }
        Ok(())
    }));
    drop(terminal);
    drop(restore);
    match result {
        Ok(result) => result.map_err(|error| error.to_string()),
        Err(payload) => std::panic::resume_unwind(payload),
    }
}
