use pkt2ai::{pkt_to_xml, xml_to_pkt};
use std::time::Instant;
use std::{fs, process::Command};

fn hex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn known_packet_pipeline() {
    let packet = hex(concat!(
        "340cb7f1fd7523b577fb6a47f61e73002c58b2e48fa5e10e93bf2adde9f25fb5",
        "64af6a55a10b39e5ad4e95b4ac4d23ab926b87943864e5ee45a7e8725257a0a6",
        "5c79596508e3e6301b8b9c1749727e49713ac57301095d1dbffb"
    ));
    let expected = b"<PACKETTRACER5><VERSION>9.0</VERSION><NETWORK><DEVICES/><LINKS/></NETWORK></PACKETTRACER5>";
    assert_eq!(pkt_to_xml(&packet).unwrap(), expected);
}

#[test]
fn round_trip_many_lengths() {
    for length in (0..=64).chain([1000, 100_000]) {
        let xml: Vec<u8> = (0..length)
            .map(|i| ((i as usize).wrapping_mul(37) & 255) as u8)
            .collect();
        let packet = xml_to_pkt(&xml).unwrap();
        assert_eq!(pkt_to_xml(&packet).unwrap(), xml, "length {length}");
    }
}

#[test]
fn rejects_random_and_tampered_data() {
    assert!(pkt_to_xml(&[0x55; 128]).is_err());
    let mut packet = xml_to_pkt(b"<PACKETTRACER5/>").unwrap();
    packet[20] ^= 1;
    assert!(pkt_to_xml(&packet).is_err());
}

#[test]
fn cross_checks_python_reference_when_available() {
    if Command::new("python3").arg("--version").output().is_err() {
        return;
    }
    let base = std::env::temp_dir().join(format!("pkt2ai-cross-{}", std::process::id()));
    let xml_path = base.with_extension("xml");
    let py_pkt = base.with_extension("python.pkt");
    let rust_pkt = base.with_extension("rust.pkt");
    let decoded = base.with_extension("decoded.xml");
    let xml = b"<PACKETTRACER5><VERSION>9.0</VERSION><NETWORK><DEVICES/><LINKS/></NETWORK></PACKETTRACER5>";
    fs::write(&xml_path, xml).unwrap();

    let root = env!("CARGO_MANIFEST_DIR");
    let status = Command::new("python3")
        .args(["pkt2ai.py", "encode"])
        .arg(&xml_path)
        .arg("-o")
        .arg(&py_pkt)
        .current_dir(root)
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(pkt_to_xml(&fs::read(&py_pkt).unwrap()).unwrap(), xml);

    fs::write(&rust_pkt, xml_to_pkt(xml).unwrap()).unwrap();
    let status = Command::new("python3")
        .args(["pkt2ai.py", "decode"])
        .arg(&rust_pkt)
        .arg("-o")
        .arg(&decoded)
        .current_dir(root)
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(fs::read(&decoded).unwrap(), xml);

    for path in [&xml_path, &py_pkt, &rust_pkt, &decoded] {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn decodes_roughly_200kb_payload() {
    let mut xml = b"<PACKETTRACER5><NETWORK><NOTE>".to_vec();
    xml.extend((0..200_000).map(|index| b'a' + (index % 23) as u8));
    xml.extend_from_slice(b"</NOTE></NETWORK></PACKETTRACER5>");
    let packet = xml_to_pkt(&xml).unwrap();
    let started = Instant::now();
    assert_eq!(pkt_to_xml(&packet).unwrap(), xml);
    eprintln!("decode 200KB XML: {:?}", started.elapsed());
}

#[test]
fn decodes_optional_real_samples() {
    let Some(directory) = std::env::var_os("PKT2AI_SAMPLES") else {
        return;
    };
    for entry in fs::read_dir(directory).unwrap().filter_map(Result::ok) {
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("pkt"))
        {
            let xml = pkt_to_xml(&fs::read(&path).unwrap())
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert!(pkt2ai::ptformat::looks_like_xml(&xml), "{}", path.display());
        }
    }
}
