//! Extract a compact, stable topology representation from Packet Tracer XML.

use regex::Regex;
use roxmltree::{Document, Node};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Info {
    pub source: String,
    pub pt_version: String,
    pub counts: Counts,
    pub devices: Vec<Device>,
    pub links: Vec<Link>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Counts {
    pub devices: usize,
    pub links: usize,
    pub by_type: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Device {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xy: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<Port>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vlans: Vec<Vlan>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interfaces: Vec<Interface>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<ConfigBlock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config_other: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub running_config: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Port {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subnet: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub dhcp: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vlan {
    pub id: u16,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Interface {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub shutdown: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub other: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigBlock {
    pub block: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub a: String,
    pub a_port: String,
    pub b: String,
    pub b_port: String,
    pub cable: String,
    pub medium: String,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug)]
pub enum ExtractError {
    Utf8(std::str::Utf8Error),
    Xml(roxmltree::Error),
}

impl fmt::Display for ExtractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Utf8(error) => write!(f, "XML khong phai UTF-8: {error}"),
            Self::Xml(error) => write!(f, "XML khong hop le: {error}"),
        }
    }
}

impl std::error::Error for ExtractError {}

fn child<'a, 'input>(node: Node<'a, 'input>, tag: &str) -> Option<Node<'a, 'input>> {
    node.children().find(|child| child.has_tag_name(tag))
}

fn path<'a, 'input>(mut node: Node<'a, 'input>, tags: &[&str]) -> Option<Node<'a, 'input>> {
    for tag in tags {
        node = child(node, tag)?;
    }
    Some(node)
}

fn text(node: Node<'_, '_>, tags: &[&str]) -> String {
    path(node, tags)
        .and_then(|value| value.text())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_default()
        .to_owned()
}

fn option(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn redact_line(line: &str, secret: &Regex) -> String {
    secret
        .replace_all(line, |caps: &regex::Captures<'_>| {
            format!("{} <redacted>", &caps[1])
        })
        .into_owned()
}

fn parse_config(lines: &[String], redact: bool) -> (Vec<Interface>, Vec<ConfigBlock>, Vec<String>) {
    let secret = Regex::new(r"(?i)\b(password|secret|community|key-string)(?:\s+\d+)?\s+\S+")
        .expect("constant regex");
    let lines: Vec<String> = lines
        .iter()
        .map(|line| {
            if redact {
                redact_line(line, &secret)
            } else {
                line.clone()
            }
        })
        .collect();
    let boilerplate = [
        "version",
        "no service timestamps log datetime msec",
        "no service timestamps debug datetime msec",
        "no service password-encryption",
        "service timestamps log datetime msec",
        "service timestamps debug datetime msec",
        "service password-encryption",
        "ip cef",
        "no ipv6 cef",
        "end",
        "spanning-tree mode pvst",
        "spanning-tree extend system-id",
    ];
    enum Current {
        None,
        Interface(usize),
        Block(usize),
    }
    let mut interfaces: Vec<Interface> = Vec::new();
    let mut blocks: Vec<ConfigBlock> = Vec::new();
    let mut other = Vec::new();
    let mut current = Current::None;
    for raw in lines {
        let trimmed_end = raw.trim_end();
        let value = trimmed_end.trim();
        if value.is_empty() || value == "!" {
            current = Current::None;
            continue;
        }
        if !trimmed_end.starts_with(' ') {
            current = Current::None;
            if let Some(name) = value.strip_prefix("interface ") {
                interfaces.push(Interface {
                    name: name.trim().to_owned(),
                    ip: None,
                    mask: None,
                    shutdown: false,
                    other: Vec::new(),
                });
                current = Current::Interface(interfaces.len() - 1);
            } else if [
                "router ",
                "ip access-list ",
                "ip dhcp pool ",
                "line ",
                "vlan ",
                "ipv6 router ",
                "crypto ",
            ]
            .iter()
            .any(|prefix| value.starts_with(prefix))
            {
                blocks.push(ConfigBlock {
                    block: value.to_owned(),
                    lines: Vec::new(),
                });
                current = Current::Block(blocks.len() - 1);
            } else if !boilerplate.contains(&value) && !value.starts_with("version ") {
                other.push(value.to_owned());
            }
            continue;
        }
        match current {
            Current::None => other.push(value.to_owned()),
            Current::Interface(index) => {
                let interface = &mut interfaces[index];
                if let Some(rest) = value.strip_prefix("ip address ") {
                    let mut bits = rest.split_whitespace();
                    interface.ip = bits.next().map(str::to_owned);
                    interface.mask = bits.next().map(str::to_owned);
                } else if value == "ip address dhcp" {
                    interface.ip = Some("dhcp".to_owned());
                } else if value == "shutdown" {
                    interface.shutdown = true;
                } else if !["no ip address", "duplex auto", "speed auto"].contains(&value) {
                    interface.other.push(value.to_owned());
                }
            }
            Current::Block(index) => blocks[index].lines.push(value.to_owned()),
        }
    }
    interfaces.retain(|interface| interface.ip.is_some() || !interface.other.is_empty());
    blocks.retain(|block| !block.lines.is_empty());
    (interfaces, blocks, other)
}

pub fn extract(
    xml: &[u8],
    source: &str,
    redact: bool,
    full_config: bool,
) -> Result<Info, ExtractError> {
    let xml = std::str::from_utf8(xml).map_err(ExtractError::Utf8)?;
    let document = Document::parse(xml).map_err(ExtractError::Xml)?;
    let root = document.root_element();
    let network = child(root, "NETWORK");
    let device_container = network.and_then(|node| child(node, "DEVICES"));
    let device_nodes: Vec<_> = device_container
        .map(|node| node.children().filter(|child| child.is_element()).collect())
        .unwrap_or_default();
    let mut warnings = Vec::new();
    if device_nodes.is_empty() {
        warnings.push("Khong tim thay NETWORK/DEVICES - cau truc XML co the da doi. Chay `pkt2ai tree file.xml` de xem cac the thuc te.".to_owned());
    }
    let secret = Regex::new(r"(?i)\b(password|secret|community|key-string)(?:\s+\d+)?\s+\S+")
        .expect("constant regex");
    let mut devices = Vec::new();
    let mut by_address = BTreeMap::new();
    let mut by_index = BTreeMap::new();
    for (index, node) in device_nodes.iter().enumerate() {
        let Some(engine) = child(*node, "ENGINE") else {
            continue;
        };
        let type_node = child(engine, "TYPE");
        let name = option(text(engine, &["NAME"])).unwrap_or_else(|| format!("device{index}"));
        let kind = type_node
            .and_then(|node| node.text())
            .unwrap_or_default()
            .trim()
            .to_owned();
        let model = type_node
            .and_then(|node| node.attribute("model"))
            .unwrap_or_default()
            .to_owned();
        let x = text(*node, &["WORKSPACE", "LOGICAL", "X"])
            .parse::<f64>()
            .ok();
        let y = text(*node, &["WORKSPACE", "LOGICAL", "Y"])
            .parse::<f64>()
            .ok();
        let xy = x.zip(y).map(|(x, y)| [x, y]);
        let port_nodes: Vec<_> = engine
            .descendants()
            .filter(|node| {
                node.has_tag_name("PORT")
                    && child(*node, "TYPE").is_some()
                    && !text(*node, &["TYPE"]).contains("Bluetooth")
            })
            .collect();
        let shown: Vec<_> = port_nodes
            .iter()
            .copied()
            .filter(|port| {
                !text(*port, &["IP"]).is_empty()
                    || text(*port, &["PORT_DHCP_ENABLE"]) == "true"
                    || port_nodes.len() <= 3
            })
            .collect();
        let ports: Vec<Port> = shown
            .iter()
            .map(|port| Port {
                kind: text(*port, &["TYPE"]),
                mac: option(text(*port, &["MACADDRESS"])),
                ip: option(text(*port, &["IP"])),
                subnet: option(text(*port, &["SUBNET"])),
                gateway: option(text(*port, &["PORT_GATEWAY"])),
                dns: option(text(*port, &["PORT_DNS"])),
                dhcp: text(*port, &["PORT_DHCP_ENABLE"]) == "true",
            })
            .collect();
        let gateway_text = text(engine, &["GATEWAY"]);
        let gateway = (!gateway_text.is_empty()
            && !ports
                .iter()
                .any(|port| port.gateway.as_deref() == Some(&gateway_text)))
        .then_some(gateway_text);
        let mut vlans = Vec::new();
        for vlan in engine
            .descendants()
            .filter(|node| node.has_tag_name("VLAN"))
        {
            let Some(id) = vlan
                .attribute("number")
                .and_then(|value| value.parse::<u16>().ok())
            else {
                continue;
            };
            let Some(vlan_name) = vlan.attribute("name") else {
                continue;
            };
            let value = Vlan {
                id,
                name: vlan_name.to_owned(),
            };
            if id < 1002 && !vlans.contains(&value) {
                vlans.push(value);
            }
        }
        let config_lines: Vec<String> = child(engine, "RUNNINGCONFIG")
            .map(|config| {
                config
                    .descendants()
                    .filter(|line| line.has_tag_name("LINE"))
                    .map(|line| line.text().unwrap_or_default().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        let (interfaces, blocks, config_other) = parse_config(&config_lines, redact);
        let running_config = full_config.then(|| {
            config_lines
                .iter()
                .map(|line| {
                    if redact {
                        redact_line(line, &secret)
                    } else {
                        line.clone()
                    }
                })
                .collect()
        });
        devices.push(Device {
            name: name.clone(),
            kind,
            model,
            xy,
            ports,
            gateway,
            vlans,
            interfaces,
            blocks,
            config_other,
            running_config,
        });
        let address = text(*node, &["WORKSPACE", "LOGICAL", "DEV_ADDR"]);
        if !address.is_empty() {
            by_address.insert(address, name.clone());
        }
        by_index.insert(index.to_string(), name);
    }
    let mut links = Vec::new();
    if let Some(link_container) = network.and_then(|node| child(node, "LINKS")) {
        for link in link_container.children().filter(|node| node.is_element()) {
            let Some(cable) = child(link, "CABLE") else {
                continue;
            };
            let ports: Vec<String> = cable
                .children()
                .filter(|node| node.has_tag_name("PORT"))
                .map(|node| node.text().unwrap_or_default().to_owned())
                .collect();
            let from_address = text(cable, &["FROM_DEVICE_MEM_ADDR"]);
            let to_address = text(cable, &["TO_DEVICE_MEM_ADDR"]);
            let from = text(cable, &["FROM"]);
            let to = text(cable, &["TO"]);
            links.push(Link {
                a: by_address
                    .get(&from_address)
                    .or_else(|| by_index.get(&from))
                    .cloned()
                    .unwrap_or(from),
                a_port: ports.first().cloned().unwrap_or_default(),
                b: by_address
                    .get(&to_address)
                    .or_else(|| by_index.get(&to))
                    .cloned()
                    .unwrap_or(to),
                b_port: ports.get(1).cloned().unwrap_or_default(),
                cable: text(cable, &["TYPE"]),
                medium: text(link, &["TYPE"]),
            });
        }
    }
    let names: std::collections::BTreeSet<_> = devices.iter().map(|device| &device.name).collect();
    let bad = links
        .iter()
        .filter(|link| !names.contains(&link.a) || !names.contains(&link.b))
        .count();
    if bad != 0 {
        warnings.push(format!(
            "{bad} link khong khop duoc ten thiet bi (ten hien thi so/dia chi)."
        ));
    }
    let mut by_type = BTreeMap::new();
    for device in &devices {
        *by_type.entry(device.kind.clone()).or_insert(0) += 1;
    }
    Ok(Info {
        source: source.to_owned(),
        pt_version: text(root, &["VERSION"]),
        counts: Counts {
            devices: devices.len(),
            links: links.len(),
            by_type,
        },
        devices,
        links,
        warnings,
    })
}

pub fn find_device<'a>(info: &'a Info, query: &str) -> Result<&'a Device, String> {
    if let Some(device) = info.devices.iter().find(|device| device.name == query) {
        return Ok(device);
    }
    let lowered = query.to_lowercase();
    if let Some(device) = info
        .devices
        .iter()
        .find(|device| device.name.to_lowercase() == lowered)
    {
        return Ok(device);
    }
    let matches: Vec<_> = info
        .devices
        .iter()
        .filter(|device| device.name.to_lowercase().contains(&lowered))
        .collect();
    match matches.as_slice() {
        [device] => Ok(device),
        [] => Err(format!(
            "khong tim thay thiet bi '{query}'. Co san: {}",
            info.devices
                .iter()
                .map(|device| device.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
        _ => Err(format!(
            "ten '{query}' mo ho: {}",
            matches
                .iter()
                .map(|device| device.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

pub fn links_of<'a>(info: &'a Info, name: &str) -> Vec<&'a Link> {
    info.links
        .iter()
        .filter(|link| link.a == name || link.b == name)
        .collect()
}

pub fn device_markdown(info: &Info, device: &Device) -> String {
    let mut out = format!(
        "## {} ({}{})\n",
        device.name,
        device.kind,
        if device.model.is_empty() {
            String::new()
        } else {
            format!(" {}", device.model)
        }
    );
    for interface in &device.interfaces {
        out.push_str(&format!(
            "- interface {}{}\n",
            interface.name,
            interface
                .ip
                .as_ref()
                .map(|ip| format!(": {ip} {}", interface.mask.as_deref().unwrap_or("")))
                .unwrap_or_default()
        ));
    }
    for line in &device.config_other {
        out.push_str(&format!("- cfg: {line}\n"));
    }
    out.push_str("\n### Lien ket\n");
    for link in links_of(info, &device.name) {
        let (own, peer, peer_port) = if link.a == device.name {
            (&link.a_port, &link.b, &link.b_port)
        } else {
            (&link.b_port, &link.a, &link.a_port)
        };
        out.push_str(&format!(
            "- {own} <-> {peer}:{peer_port} ({})\n",
            link.cable
        ));
    }
    out
}

pub fn to_markdown(info: &Info) -> String {
    let types = info
        .counts
        .by_type
        .iter()
        .map(|(kind, count)| format!("{kind} x{count}"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut out = format!(
        "# Packet Tracer: {} (PT {})\n{} thiet bi, {} lien ket. Theo loai: {types}\n\n## Thiet bi\n",
        info.source, info.pt_version, info.counts.devices, info.counts.links
    );
    for device in &info.devices {
        out.push_str(&format!(
            "- **{}** ({}{})\n",
            device.name,
            device.kind,
            if device.model.is_empty() {
                String::new()
            } else {
                format!(" {}", device.model)
            }
        ));
        if let Some(gateway) = &device.gateway {
            out.push_str(&format!("  - gateway: {gateway}\n"));
        }
        for port in &device.ports {
            let mut values = Vec::new();
            if let Some(mac) = &port.mac {
                values.push(format!("mac={mac}"));
            }
            if let Some(ip) = &port.ip {
                values.push(format!("ip={ip}"));
            }
            if let Some(subnet) = &port.subnet {
                values.push(format!("subnet={subnet}"));
            }
            if let Some(gateway) = &port.gateway {
                values.push(format!("gateway={gateway}"));
            }
            if let Some(dns) = &port.dns {
                values.push(format!("dns={dns}"));
            }
            if port.dhcp {
                values.push("dhcp=true".to_owned());
            }
            if !values.is_empty() {
                out.push_str(&format!("  - port {}: {}\n", port.kind, values.join(", ")));
            }
        }
        if !device.vlans.is_empty() {
            out.push_str(&format!(
                "  - VLAN: {}\n",
                device
                    .vlans
                    .iter()
                    .map(|vlan| format!("{}({})", vlan.id, vlan.name))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        for interface in &device.interfaces {
            let mut line = format!("  - interface {}", interface.name);
            if let Some(ip) = &interface.ip {
                line.push_str(&format!(
                    ": {ip}{}",
                    interface
                        .mask
                        .as_ref()
                        .map(|mask| format!(" {mask}"))
                        .unwrap_or_default()
                ));
            }
            if interface.shutdown {
                line.push_str(" [shutdown]");
            }
            if !interface.other.is_empty() {
                line.push_str(&format!(" | {}", interface.other.join("; ")));
            }
            out.push_str(&line);
            out.push('\n');
        }
        for block in &device.blocks {
            out.push_str(&format!(
                "  - {}: {}\n",
                block.block,
                block.lines.join("; ")
            ));
        }
        for line in &device.config_other {
            out.push_str(&format!("  - cfg: {line}\n"));
        }
    }
    out.push_str("\n## Lien ket\n");
    for link in &info.links {
        out.push_str(&format!(
            "- {}:{} <-> {}:{} ({})\n",
            link.a, link.a_port, link.b, link.b_port, link.cable
        ));
    }
    for warning in &info.warnings {
        out.push_str(&format!("\n[!] {warning}\n"));
    }
    out
}

pub fn tree_outline(xml: &[u8], depth: usize) -> Result<String, ExtractError> {
    let xml = std::str::from_utf8(xml).map_err(ExtractError::Utf8)?;
    let document = Document::parse(xml).map_err(ExtractError::Xml)?;
    let mut paths: Vec<(String, usize)> = Vec::new();
    fn walk(
        node: Node<'_, '_>,
        current: String,
        level: usize,
        depth: usize,
        paths: &mut Vec<(String, usize)>,
    ) {
        if level > depth {
            return;
        }
        if let Some((_, count)) = paths.iter_mut().find(|(path, _)| path == &current) {
            *count += 1;
        } else {
            paths.push((current.clone(), 1));
        }
        for child in node.children().filter(|child| child.is_element()) {
            walk(
                child,
                format!("{current}/{}", child.tag_name().name()),
                level + 1,
                depth,
                paths,
            );
        }
    }
    let root = document.root_element();
    walk(
        root,
        root.tag_name().name().to_owned(),
        0,
        depth,
        &mut paths,
    );
    Ok(paths
        .into_iter()
        .map(|(path, count)| format!("{count:>6}  {path}\n"))
        .collect())
}
