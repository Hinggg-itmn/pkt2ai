//! Terminal-independent topology exporters.

use crate::{
    Info, device_markdown,
    graph::{Graph, node_style},
    layout::{Point, bounds},
};
use std::collections::{BTreeMap, BTreeSet};

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn dot_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

pub fn dot(graph: &Graph) -> String {
    let mut out = String::from(
        "graph topology {\n  graph [overlap=false, splines=true];\n  node [style=filled];\n",
    );
    for (index, node) in graph.nodes.iter().enumerate() {
        out.push_str(&format!(
            "  n{index} [label=\"{}\", fillcolor=\"{}\"];\n",
            dot_escape(&node.name),
            node_style(&node.kind).color
        ));
    }
    for edge in &graph.edges {
        if let Some((a, b)) = edge.a.zip(edge.b) {
            out.push_str(&format!(
                "  n{a} -- n{b} [label=\"{} ↔ {}\"];\n",
                dot_escape(&edge.link.a_port),
                dot_escape(&edge.link.b_port)
            ));
        }
    }
    out.push_str("}\n");
    out
}

pub fn mermaid(graph: &Graph) -> String {
    let mut out = String::from("graph LR\n");
    for (index, node) in graph.nodes.iter().enumerate() {
        out.push_str(&format!(
            "  n{index}[\"{}\"]\n",
            node.name.replace('"', "&quot;")
        ));
    }
    for edge in &graph.edges {
        if let Some((a, b)) = edge.a.zip(edge.b) {
            out.push_str(&format!(
                "  n{a} ---|\"{} / {}\"| n{b}\n",
                edge.link.a_port.replace('"', "&quot;"),
                edge.link.b_port.replace('"', "&quot;")
            ));
        }
    }
    out
}

pub fn graphml(graph: &Graph) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\">\n  <key id=\"name\" for=\"node\" attr.name=\"name\" attr.type=\"string\"/>\n  <key id=\"type\" for=\"node\" attr.name=\"type\" attr.type=\"string\"/>\n  <key id=\"ports\" for=\"edge\" attr.name=\"ports\" attr.type=\"string\"/>\n  <graph id=\"topology\" edgedefault=\"undirected\">\n",
    );
    for (index, node) in graph.nodes.iter().enumerate() {
        out.push_str(&format!("    <node id=\"n{index}\"><data key=\"name\">{}</data><data key=\"type\">{}</data></node>\n", xml_escape(&node.name), xml_escape(&node.kind)));
    }
    for (index, edge) in graph.edges.iter().enumerate() {
        if let Some((a, b)) = edge.a.zip(edge.b) {
            out.push_str(&format!("    <edge id=\"e{index}\" source=\"n{a}\" target=\"n{b}\"><data key=\"ports\">{} ↔ {}</data></edge>\n", xml_escape(&edge.link.a_port), xml_escape(&edge.link.b_port)));
        }
    }
    out.push_str("  </graph>\n</graphml>\n");
    out
}

pub fn svg(graph: &Graph, positions: &[Point], width: u32, height: u32) -> String {
    let [min_x, max_x, min_y, max_y] = bounds(positions);
    let margin = 45.0;
    let draw_w = (f64::from(width) - 2.0 * margin).max(1.0);
    let draw_h = (f64::from(height) - 2.0 * margin).max(1.0);
    let map = |point: Point| {
        [
            margin + (point[0] - min_x) / (max_x - min_x) * draw_w,
            margin + (point[1] - min_y) / (max_y - min_y) * draw_h,
        ]
    };
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\"><rect width=\"100%\" height=\"100%\" fill=\"#171923\"/><g stroke=\"#64748b\" fill=\"none\">\n"
    );
    for edge in &graph.edges {
        let Some((a, b)) = edge.a.zip(edge.b) else {
            continue;
        };
        if a >= positions.len() || b >= positions.len() {
            continue;
        }
        let from = map(positions[a]);
        let to = map(positions[b]);
        let dx = to[0] - from[0];
        let dy = to[1] - from[1];
        let length = dx.hypot(dy).max(1.0);
        let offset = (edge.parallel_index as f64
            - (edge.parallel_count.saturating_sub(1)) as f64 / 2.0)
            * 8.0;
        let ox = -dy / length * offset;
        let oy = dx / length * offset;
        out.push_str(&format!(
            "<line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\"/>\n",
            from[0] + ox,
            from[1] + oy,
            to[0] + ox,
            to[1] + oy
        ));
    }
    out.push_str("</g><g font-family=\"sans-serif\" font-size=\"12\" text-anchor=\"middle\">\n");
    for (index, node) in graph.nodes.iter().enumerate() {
        let Some(position) = positions.get(index).copied() else {
            continue;
        };
        let point = map(position);
        let style = node_style(&node.kind);
        out.push_str(&format!("<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"8\" fill=\"{}\"/><text x=\"{:.2}\" y=\"{:.2}\" fill=\"#f8fafc\">{}</text>\n", point[0], point[1], style.color, point[0], point[1] - 13.0, xml_escape(&node.name)));
    }
    out.push_str("</g></svg>\n");
    out
}

fn safe_stem(name: &str) -> String {
    let mut stem: String = name
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, ' ' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect();
    stem = stem.trim_matches([' ', '.']).to_owned();
    if stem.is_empty() || stem == "." || stem == ".." {
        "device".to_owned()
    } else {
        stem
    }
}

fn yaml(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Return safe relative filenames and note contents for an Obsidian vault.
pub fn obsidian(info: &Info) -> Vec<(String, String)> {
    let mut used = BTreeSet::new();
    let mut names = BTreeMap::new();
    for device in &info.devices {
        let base = safe_stem(&device.name);
        let mut stem = base.clone();
        let mut suffix = 2;
        while !used.insert(stem.to_lowercase()) {
            stem = format!("{base}-{suffix}");
            suffix += 1;
        }
        names.insert(device.name.clone(), stem);
    }
    info.devices
        .iter()
        .map(|device| {
            let ips: Vec<_> = device
                .ports
                .iter()
                .filter_map(|port| port.ip.as_deref())
                .chain(
                    device
                        .interfaces
                        .iter()
                        .filter_map(|interface| interface.ip.as_deref()),
                )
                .collect();
            let vlans: Vec<_> = device
                .vlans
                .iter()
                .map(|vlan| vlan.id.to_string())
                .collect();
            let mut note = format!(
                "---\ntype: {}\nmodel: {}\nips: [{}]\nvlans: [{}]\n---\n\n# {}\n\n",
                yaml(&device.kind),
                yaml(&device.model),
                ips.iter().map(|ip| yaml(ip)).collect::<Vec<_>>().join(", "),
                vlans.join(", "),
                device.name
            );
            note.push_str(&device_markdown(info, device));
            note.push_str("\n## Liên kết\n");
            for link in info
                .links
                .iter()
                .filter(|link| link.a == device.name || link.b == device.name)
            {
                let (own_port, peer, peer_port) = if link.a == device.name {
                    (&link.a_port, &link.b, &link.b_port)
                } else {
                    (&link.b_port, &link.a, &link.a_port)
                };
                if let Some(peer_stem) = names.get(peer) {
                    note.push_str(&format!(
                        "- {own_port} ↔ [[{peer_stem}|{peer}]]:{peer_port}\n"
                    ));
                }
            }
            (format!("{}.md", names[&device.name]), note)
        })
        .collect()
}
