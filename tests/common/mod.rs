use std::{fs, path::PathBuf};

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

pub fn temp_file(label: &str, content: &[u8]) -> PathBuf {
    let path = std::env::temp_dir().join(format!("pkt2ai-{label}-{}", std::process::id()));
    fs::write(&path, content).unwrap();
    path
}
