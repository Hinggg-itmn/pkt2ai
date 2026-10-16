mod common;

use common::{SAMPLE_XML, temp_file};
use pkt2ai::{extract, find_device, links_of, tree_outline};
use std::{
    fs,
    process::{Command, Stdio},
};

#[test]
fn extracts_devices_config_and_parallel_or_unknown_links() {
    let info = extract(SAMPLE_XML.as_bytes(), "fixture.xml", true, true).unwrap();
    assert_eq!(info.counts.devices, 3);
    assert_eq!(info.counts.links, 4);
    assert_eq!(
        info.links
            .iter()
            .filter(|link| link.a == "Switch - tang1" && link.b == "R&D")
            .count(),
        2
    );
    assert_eq!(info.links.last().unwrap().a, "99");
    assert_eq!(info.warnings.len(), 1);
    let switch = find_device(&info, "switch").unwrap();
    assert_eq!(switch.vlans.len(), 1);
    assert_eq!(switch.interfaces[0].ip.as_deref(), Some("10.0.0.3"));
    assert!(switch.config_other[0].contains("<redacted>"));
    assert_eq!(links_of(&info, &switch.name).len(), 3);
    assert!(find_device(&info, "no such node").is_err());
    assert!(
        tree_outline(SAMPLE_XML.as_bytes(), 3)
            .unwrap()
            .contains("PACKETTRACER5/NETWORK/DEVICES/DEVICE")
    );
}

#[test]
fn cli_commands_and_exit_codes() {
    let fixture = temp_file("cli.xml", SAMPLE_XML.as_bytes());
    let binary = env!("CARGO_BIN_EXE_pkt2ai");
    let summary = Command::new(binary)
        .args(["summary", fixture.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(summary.status.success());
    let value: serde_json::Value = serde_json::from_slice(&summary.stdout).unwrap();
    assert_eq!(value["counts"]["devices"], 3);
    let list = Command::new(binary)
        .args(["list", fixture.to_str().unwrap(), "--type", "switch"])
        .output()
        .unwrap();
    assert!(
        String::from_utf8(list.stdout)
            .unwrap()
            .contains("Switch - tang1")
    );
    let show = Command::new(binary)
        .args(["show", fixture.to_str().unwrap(), "r&d"])
        .output()
        .unwrap();
    assert!(
        String::from_utf8(show.stdout)
            .unwrap()
            .contains("GigabitEthernet0/0")
    );
    let links = Command::new(binary)
        .args(["links", fixture.to_str().unwrap(), "--device", "PC 1"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8(links.stdout).unwrap().lines().count(), 1);
    let found = Command::new(binary)
        .args(["grep", fixture.to_str().unwrap(), "OSPF", "-i"])
        .output()
        .unwrap();
    assert!(found.status.success());
    let missing = Command::new(binary)
        .args(["grep", fixture.to_str().unwrap(), "definitely-absent"])
        .status()
        .unwrap();
    assert_eq!(missing.code(), Some(3));
    let bad = Command::new(binary).arg("not-a-command").status().unwrap();
    assert_eq!(bad.code(), Some(2));
    let _ = fs::remove_file(fixture);
}

#[test]
fn summary_matches_python_reference_when_available() {
    if Command::new("python3").arg("--version").output().is_err() {
        return;
    }
    let fixture = temp_file("cross-summary.xml", SAMPLE_XML.as_bytes());
    let rust = Command::new(env!("CARGO_BIN_EXE_pkt2ai"))
        .args(["summary", fixture.to_str().unwrap()])
        .output()
        .unwrap();
    let python = Command::new("python3")
        .args(["pkt2ai.py", "summary", fixture.to_str().unwrap()])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        rust.status.success(),
        "{}",
        String::from_utf8_lossy(&rust.stderr)
    );
    assert!(
        python.status.success(),
        "{}",
        String::from_utf8_lossy(&python.stderr)
    );
    let rust: serde_json::Value = serde_json::from_slice(&rust.stdout).unwrap();
    let python: serde_json::Value = serde_json::from_slice(&python.stdout).unwrap();
    assert_eq!(rust, python);
    let _ = fs::remove_file(fixture);
}

#[cfg(unix)]
#[test]
fn broken_pipe_exits_cleanly() {
    let mut xml = String::from("<PACKETTRACER5><VERSION>9</VERSION><NETWORK><DEVICES>");
    for index in 0..5000 {
        xml.push_str(&format!(
            "<DEVICE><ENGINE><NAME>device-{index}</NAME><TYPE>Pc</TYPE></ENGINE></DEVICE>"
        ));
    }
    xml.push_str("</DEVICES><LINKS/></NETWORK></PACKETTRACER5>");
    let fixture = temp_file("broken-pipe.xml", xml.as_bytes());
    let mut child = Command::new(env!("CARGO_BIN_EXE_pkt2ai"))
        .args(["summary", fixture.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = fs::remove_file(fixture);
}
