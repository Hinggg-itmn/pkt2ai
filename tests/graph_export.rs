mod common;

use common::{SAMPLE_XML, temp_file};
use pkt2ai::{
    Counts, Device, Info,
    export::{dot, graphml, mermaid, obsidian, svg},
    extract,
    graph::Graph,
    layout::force_directed,
};
use std::{collections::BTreeMap, fs, process::Command, time::Instant};

fn empty_info() -> Info {
    Info {
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
    }
}

fn generated_graph(count: usize) -> Graph {
    let mut info = empty_info();
    info.devices = (0..count)
        .map(|index| Device {
            name: format!("node-{index}"),
            kind: if index % 2 == 0 { "Router" } else { "Switch" }.to_owned(),
            model: String::new(),
            xy: None,
            ports: Vec::new(),
            gateway: None,
            vlans: Vec::new(),
            interfaces: Vec::new(),
            blocks: Vec::new(),
            config_other: Vec::new(),
            running_config: None,
        })
        .collect();
    info.links = (1..count)
        .map(|index| pkt2ai::Link {
            a: format!("node-{}", index - 1),
            a_port: "a".to_owned(),
            b: format!("node-{index}"),
            b_port: "b".to_owned(),
            cable: "copper".to_owned(),
            medium: String::new(),
        })
        .collect();
    Graph::new(&info)
}

#[test]
fn layout_is_reproducible_finite_and_distinct() {
    for count in [0, 1, 50, 500] {
        let graph = generated_graph(count);
        let started = Instant::now();
        let first = force_directed(&graph, 42, if count == 500 { 50 } else { 100 });
        let elapsed = started.elapsed();
        let second = force_directed(&graph, 42, if count == 500 { 50 } else { 100 });
        assert_eq!(first, second);
        assert!(first.iter().flatten().all(|value| value.is_finite()));
        for left in 0..first.len() {
            for right in left + 1..first.len() {
                assert_ne!(first[left], first[right]);
            }
        }
        eprintln!("layout {count}: {elapsed:?}");
    }
}

#[test]
fn local_graph_obeys_hop_count() {
    let graph = generated_graph(6);
    assert_eq!(graph.local_nodes(2, 0).len(), 1);
    assert_eq!(graph.local_nodes(2, 1).len(), 3);
    assert_eq!(graph.local_nodes(2, 2).len(), 5);
}

#[test]
fn exports_are_valid_and_unknown_links_do_not_crash() {
    let info = extract(SAMPLE_XML.as_bytes(), "fixture", true, false).unwrap();
    let graph = Graph::new(&info);
    let positions = force_directed(&graph, 7, 40);
    let dot_text = dot(&graph);
    assert!(dot_text.starts_with("graph topology"));
    assert_eq!(dot_text.matches("n1 -- n2").count(), 2);
    let mermaid_text = mermaid(&graph);
    assert!(mermaid_text.starts_with("graph LR"));
    assert_eq!(mermaid_text.matches("n1 ---").count(), 2);
    roxmltree::Document::parse(&graphml(&graph)).unwrap();
    roxmltree::Document::parse(&svg(&graph, &positions, 800, 600)).unwrap();
    let empty = Graph::new(&empty_info());
    roxmltree::Document::parse(&svg(&empty, &[], 80, 24)).unwrap();
}

#[test]
fn obsidian_links_both_ways_and_sanitizes_collisions() {
    let mut info = extract(SAMPLE_XML.as_bytes(), "fixture", true, false).unwrap();
    let template = info.devices[0].clone();
    let mut first = template.clone();
    first.name = "a/b".to_owned();
    let mut second = template;
    second.name = "a?b".to_owned();
    info.devices.extend([first, second]);
    let files = obsidian(&info);
    let names: Vec<_> = files.iter().map(|(name, _)| name).collect();
    assert!(names.contains(&&"R_D.md".to_owned()));
    assert!(names.contains(&&"Switch - tang1.md".to_owned()));
    assert!(names.contains(&&"a_b.md".to_owned()));
    assert!(names.contains(&&"a_b-2.md".to_owned()));
    assert!(names.iter().all(|name| !name.contains('/')));
    let pc = files.iter().find(|(name, _)| name == "PC 1.md").unwrap();
    let switch = files
        .iter()
        .find(|(name, _)| name == "Switch - tang1.md")
        .unwrap();
    assert!(pc.1.contains("[[Switch - tang1|Switch - tang1]]"));
    assert!(switch.1.contains("[[PC 1|PC 1]]"));
}

#[test]
fn export_cli_refuses_overwrite_without_force() {
    let fixture = temp_file("export.xml", SAMPLE_XML.as_bytes());
    let output = temp_file("export.dot", b"keep me");
    let binary = env!("CARGO_BIN_EXE_pkt2ai");
    let refused = Command::new(binary)
        .args([
            "export",
            fixture.to_str().unwrap(),
            "--format",
            "dot",
            "-o",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(1));
    assert_eq!(fs::read(&output).unwrap(), b"keep me");
    let replaced = Command::new(binary)
        .args([
            "export",
            fixture.to_str().unwrap(),
            "--format",
            "dot",
            "-o",
            output.to_str().unwrap(),
            "--force",
        ])
        .status()
        .unwrap();
    assert!(replaced.success());
    assert!(
        fs::read_to_string(&output)
            .unwrap()
            .starts_with("graph topology")
    );
    let _ = fs::remove_file(fixture);
    let _ = fs::remove_file(output);
}
