use std::{fs, path::PathBuf};

#[allow(dead_code)]
pub const SAMPLE_XML: &str = r#"<PACKETTRACER5>
<VERSION>9.0.1</VERSION><NETWORK><DEVICES>
<DEVICE><ENGINE><NAME>PC 1</NAME><TYPE model="PC-PT">Pc</TYPE><GATEWAY>10.0.0.1</GATEWAY>
<PORT><TYPE>FastEthernet0</TYPE><MACADDRESS>0001.0203.0405</MACADDRESS><IP>10.0.0.2</IP><SUBNET>255.255.255.0</SUBNET><PORT_GATEWAY>10.0.0.1</PORT_GATEWAY></PORT>
<RUNNINGCONFIG><LINE>version 15.1</LINE><LINE>enable secret swordfish</LINE></RUNNINGCONFIG></ENGINE>
<WORKSPACE><LOGICAL><X>10</X><Y>20</Y><DEV_ADDR>pcaddr</DEV_ADDR></LOGICAL></WORKSPACE></DEVICE>
<DEVICE><ENGINE><NAME>Switch - tang1</NAME><TYPE model="2960">Switch</TYPE>
<VLANS><VLAN number="10" name="Students"/><VLAN number="10" name="Students"/><VLAN number="1002" name="fddi-default"/></VLANS>
<RUNNINGCONFIG><LINE>interface Vlan10</LINE><LINE> ip address 10.0.0.3 255.255.255.0</LINE><LINE>!</LINE><LINE>snmp-server community public ro</LINE></RUNNINGCONFIG></ENGINE>
<WORKSPACE><LOGICAL><X>100</X><Y>20</Y><DEV_ADDR>swaddr</DEV_ADDR></LOGICAL></WORKSPACE></DEVICE>
<DEVICE><ENGINE><NAME>R&amp;D</NAME><TYPE model="2911">Router</TYPE>
<RUNNINGCONFIG><LINE>interface GigabitEthernet0/0</LINE><LINE> ip address 10.0.0.1 255.255.255.0</LINE><LINE>!</LINE><LINE>router ospf 1</LINE><LINE> network 10.0.0.0 0.0.0.255 area 0</LINE></RUNNINGCONFIG></ENGINE>
<WORKSPACE><LOGICAL><X>200</X><Y>20</Y><DEV_ADDR>raddr</DEV_ADDR></LOGICAL></WORKSPACE></DEVICE>
</DEVICES><LINKS>
<LINK><TYPE>copper</TYPE><CABLE><FROM>0</FROM><TO>1</TO><PORT>FastEthernet0</PORT><PORT>FastEthernet0/1</PORT><FROM_DEVICE_MEM_ADDR>pcaddr</FROM_DEVICE_MEM_ADDR><TO_DEVICE_MEM_ADDR>swaddr</TO_DEVICE_MEM_ADDR><TYPE>straight-through</TYPE></CABLE></LINK>
<LINK><TYPE>copper</TYPE><CABLE><FROM>1</FROM><TO>2</TO><PORT>FastEthernet0/2</PORT><PORT>GigabitEthernet0/0</PORT><FROM_DEVICE_MEM_ADDR>swaddr</FROM_DEVICE_MEM_ADDR><TO_DEVICE_MEM_ADDR>raddr</TO_DEVICE_MEM_ADDR><TYPE>straight-through</TYPE></CABLE></LINK>
<LINK><TYPE>copper</TYPE><CABLE><FROM>1</FROM><TO>2</TO><PORT>FastEthernet0/3</PORT><PORT>GigabitEthernet0/1</PORT><TYPE>cross-over</TYPE></CABLE></LINK>
<LINK><TYPE>serial</TYPE><CABLE><FROM>99</FROM><TO>2</TO><PORT>Serial0</PORT><PORT>Serial0/0</PORT><TYPE>serial</TYPE></CABLE></LINK>
</LINKS></NETWORK></PACKETTRACER5>"#;

#[allow(dead_code)]
pub fn temp_file(label: &str, content: &[u8]) -> PathBuf {
    let path = std::env::temp_dir().join(format!("pkt2ai-{label}-{}", std::process::id()));
    fs::write(&path, content).unwrap();
    path
}

/// Cây campus 48 thiết bị, bốn tầng dùng để đo khả năng đọc của TUI:
/// router → hai core → mười access switch → ba mươi lăm endpoint.
#[allow(dead_code)]
pub fn layered_info_48() -> pkt2ai::Info {
    let mut xml = String::from("<PACKETTRACER5><VERSION>9.0.1</VERSION><NETWORK><DEVICES>");
    for index in 0..48 {
        let (name, kind, model) = match index {
            0 => ("Router-HQ".to_owned(), "Router", "ISR4331"),
            1..=2 => (format!("Core-{index}"), "MultiLayerSwitch", "3650"),
            3..=12 => (format!("Switch-Phong-{:02}", index - 2), "Switch", "2960"),
            _ => {
                let endpoint = index - 12;
                let (kind, model) = match endpoint % 5 {
                    0 => ("Laptop", "Laptop-PT"),
                    1 => ("Server", "Server-PT"),
                    2 => ("AccessPoint", "AP-PT"),
                    3 => ("Printer", "Printer-PT"),
                    _ => ("Pc", "PC-PT"),
                };
                (format!("{kind}-Phong-{endpoint:02}"), kind, model)
            }
        };
        xml.push_str(&format!(
            "<DEVICE><ENGINE><NAME>{name}</NAME><TYPE model=\"{model}\">{kind}</TYPE>"
        ));
        if index == 0 || index >= 13 {
            xml.push_str(&format!(
                "<PORT><TYPE>GigabitEthernet0/{}</TYPE><IP>10.{}.{}.2</IP><SUBNET>255.255.255.0</SUBNET></PORT>",
                index % 4,
                index / 250,
                index % 250
            ));
        }
        xml.push_str("</ENGINE>");
        // Cố ý trộn thiết bị có/không có tọa độ Packet Tracer để kiểm tra
        // fallback của layout PT trên dữ liệu không đầy đủ.
        if index % 2 == 0 {
            let (x, y) = if index == 0 {
                (500, 40)
            } else if index <= 2 {
                (250 + (index - 1) * 500, 140)
            } else if index <= 12 {
                (80 + (index - 3) * 95, 260)
            } else {
                (30 + (index - 13) * 28, 420)
            };
            xml.push_str(&format!(
                "<WORKSPACE><LOGICAL><X>{x}</X><Y>{y}</Y><DEV_ADDR>dev{index}</DEV_ADDR></LOGICAL></WORKSPACE>"
            ));
        }
        xml.push_str("</DEVICE>");
    }
    xml.push_str("</DEVICES><LINKS>");
    for child in 1..48 {
        let parent = match child {
            1..=2 => 0,
            3..=12 => 1 + (child - 3) % 2,
            _ => 3 + (child - 13) % 10,
        };
        xml.push_str(&format!(
            "<LINK><TYPE>copper</TYPE><CABLE><FROM>{parent}</FROM><TO>{child}</TO><PORT>GigabitEthernet1/0/{child}</PORT><PORT>FastEthernet0/1</PORT><TYPE>straight-through</TYPE></CABLE></LINK>"
        ));
    }
    xml.push_str("</LINKS></NETWORK></PACKETTRACER5>");
    pkt2ai::extract(xml.as_bytes(), "layered-48.xml", true, false).unwrap()
}
