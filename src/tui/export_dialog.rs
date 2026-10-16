#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiExportFormat {
    Json,
    Markdown,
    Xml,
    Pkt,
    Obsidian,
    Dot,
    Mermaid,
    Graphml,
    Svg,
}

impl TuiExportFormat {
    pub const ALL: [Self; 9] = [
        Self::Json,
        Self::Markdown,
        Self::Xml,
        Self::Pkt,
        Self::Obsidian,
        Self::Dot,
        Self::Mermaid,
        Self::Graphml,
        Self::Svg,
    ];
    pub const fn label(self) -> &'static str {
        match self {
            Self::Json => "JSON",
            Self::Markdown => "Markdown",
            Self::Xml => "XML decoded",
            Self::Pkt => "Packet Tracer .pkt",
            Self::Obsidian => "Obsidian vault",
            Self::Dot => "Graphviz DOT",
            Self::Mermaid => "Mermaid",
            Self::Graphml => "GraphML",
            Self::Svg => "SVG",
        }
    }
    pub const fn needs_xml(self) -> bool {
        matches!(self, Self::Xml | Self::Pkt)
    }
}

#[derive(Debug, Clone)]
pub struct ExportDialog {
    pub selected: usize,
    pub path: String,
    pub editing_path: bool,
    pub visible_only: bool,
}
impl ExportDialog {
    pub fn new(path: String) -> Self {
        Self {
            selected: 0,
            path,
            editing_path: false,
            visible_only: false,
        }
    }
    pub fn format(&self) -> TuiExportFormat {
        TuiExportFormat::ALL[self.selected]
    }
    pub fn move_by(&mut self, delta: isize) {
        self.selected = self
            .selected
            .saturating_add_signed(delta)
            .min(TuiExportFormat::ALL.len() - 1);
    }
}
