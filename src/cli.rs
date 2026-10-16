//! Command-line interface and stable process exit semantics.

use clap::{Parser, Subcommand, ValueEnum};
use regex::RegexBuilder;
use std::{
    ffi::OsStr,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use crate::{
    Device, Info, extract, find_device, graph::Graph, layout::force_directed, links_of, pkt_to_xml,
    pkt_to_xml_with_verify, to_markdown, tree_outline, xml_to_pkt,
};

#[derive(Debug, Parser)]
#[command(
    name = "pkt2ai",
    version,
    about = "Decode and inspect Cisco Packet Tracer files"
)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Decode .pkt to XML.
    Decode {
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long)]
        no_verify: bool,
    },
    /// Encode XML to .pkt.
    Encode {
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Extract a compact JSON or Markdown summary.
    Summary {
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = SummaryFormat::Json)]
        format: SummaryFormat,
        #[arg(long)]
        full_config: bool,
        #[arg(long)]
        keep_secrets: bool,
    },
    /// Print XML tag paths for format debugging.
    Tree {
        input: PathBuf,
        #[arg(long, default_value_t = 4)]
        depth: usize,
    },
    /// List devices in a compact table.
    List {
        input: PathBuf,
        #[arg(long = "type")]
        kind: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Show one device and all of its links.
    Show {
        input: PathBuf,
        device: String,
        #[arg(long)]
        json: bool,
    },
    /// List topology links.
    Links {
        input: PathBuf,
        #[arg(long)]
        device: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Search configuration lines with a regular expression.
    Grep {
        input: PathBuf,
        regex: String,
        #[arg(short = 'i', long)]
        ignore_case: bool,
        #[arg(long)]
        device: Option<String>,
    },
    /// Export the topology for graph tools or an Obsidian vault.
    Export {
        input: PathBuf,
        #[arg(long, value_enum)]
        format: ExportFormat,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        force: bool,
    },
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum SummaryFormat {
    Json,
    Md,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum ExportFormat {
    Dot,
    Mermaid,
    Graphml,
    Svg,
    Obsidian,
}

#[derive(Debug)]
struct CliError {
    message: String,
    code: u8,
}

impl CliError {
    fn ordinary(error: impl std::fmt::Display) -> Self {
        Self {
            message: error.to_string(),
            code: 1,
        }
    }
    fn grep_empty() -> Self {
        Self {
            message: String::new(),
            code: 3,
        }
    }
}

fn read(path: &Path) -> Result<Vec<u8>, CliError> {
    if path == OsStr::new("-") {
        let mut data = Vec::new();
        io::stdin()
            .read_to_end(&mut data)
            .map_err(CliError::ordinary)?;
        Ok(data)
    } else {
        fs::read(path).map_err(CliError::ordinary)
    }
}

fn source_name(path: &Path) -> String {
    if path == OsStr::new("-") {
        "stdin".to_owned()
    } else {
        path.file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy()
            .into_owned()
    }
}

fn load_xml_bytes(path: &Path) -> Result<Vec<u8>, CliError> {
    let data = read(path)?;
    if crate::ptformat::looks_like_xml(&data) {
        Ok(data)
    } else {
        pkt_to_xml(&data).map_err(CliError::ordinary)
    }
}

fn load_info(path: &Path, redact: bool, full_config: bool) -> Result<Info, CliError> {
    let data = read(path)?;
    if data.iter().find(|byte| !byte.is_ascii_whitespace()) == Some(&b'{') {
        serde_json::from_slice(&data).map_err(CliError::ordinary)
    } else {
        let xml = if crate::ptformat::looks_like_xml(&data) {
            data
        } else {
            pkt_to_xml(&data).map_err(CliError::ordinary)?
        };
        extract(&xml, &source_name(path), redact, full_config).map_err(CliError::ordinary)
    }
}

fn default_output(input: &Path, extension: &str) -> PathBuf {
    let mut output = input.to_path_buf();
    output.set_extension(extension);
    output
}

fn device_ips(device: &Device) -> String {
    device
        .ports
        .iter()
        .filter_map(|port| port.ip.as_deref())
        .chain(
            device
                .interfaces
                .iter()
                .filter_map(|interface| interface.ip.as_deref()),
        )
        .collect::<Vec<_>>()
        .join(",")
}

fn execute(command: Command) -> Result<Option<Vec<u8>>, CliError> {
    match command {
        Command::Decode {
            input,
            output,
            no_verify,
        } => {
            let xml =
                pkt_to_xml_with_verify(&read(&input)?, !no_verify).map_err(CliError::ordinary)?;
            if let Some(path) =
                output.or_else(|| (input != OsStr::new("-")).then(|| default_output(&input, "xml")))
            {
                fs::write(&path, &xml).map_err(CliError::ordinary)?;
                Ok(Some(
                    format!(
                        "[+] {} -> {} ({} byte)\n",
                        input.display(),
                        path.display(),
                        xml.len()
                    )
                    .into_bytes(),
                ))
            } else {
                Ok(Some(xml))
            }
        }
        Command::Encode { input, output } => {
            let packet = xml_to_pkt(&read(&input)?).map_err(CliError::ordinary)?;
            if let Some(path) =
                output.or_else(|| (input != OsStr::new("-")).then(|| default_output(&input, "pkt")))
            {
                fs::write(&path, &packet).map_err(CliError::ordinary)?;
                Ok(Some(
                    format!("[+] {} -> {}\n", input.display(), path.display()).into_bytes(),
                ))
            } else {
                Ok(Some(packet))
            }
        }
        Command::Summary {
            input,
            output,
            format,
            full_config,
            keep_secrets,
        } => {
            let info = load_info(&input, !keep_secrets, full_config)?;
            let text = match format {
                SummaryFormat::Json => format!(
                    "{}\n",
                    serde_json::to_string_pretty(&info).map_err(CliError::ordinary)?
                ),
                SummaryFormat::Md => to_markdown(&info),
            };
            if let Some(path) = output {
                fs::write(&path, text.as_bytes()).map_err(CliError::ordinary)?;
                Ok(Some(
                    format!(
                        "[+] {} -> {} ({} ky tu)\n",
                        input.display(),
                        path.display(),
                        text.len()
                    )
                    .into_bytes(),
                ))
            } else {
                Ok(Some(text.into_bytes()))
            }
        }
        Command::Tree { input, depth } => Ok(Some(
            tree_outline(&load_xml_bytes(&input)?, depth)
                .map_err(CliError::ordinary)?
                .into_bytes(),
        )),
        Command::List { input, kind, json } => {
            let info = load_info(&input, true, false)?;
            let devices: Vec<_> = info
                .devices
                .iter()
                .filter(|device| {
                    kind.as_ref()
                        .is_none_or(|kind| device.kind.eq_ignore_ascii_case(kind))
                })
                .collect();
            if json {
                return Ok(Some(
                    format!(
                        "{}\n",
                        serde_json::to_string_pretty(&devices).map_err(CliError::ordinary)?
                    )
                    .into_bytes(),
                ));
            }
            let mut text = "NAME\tTYPE\tMODEL\tIP\tLINKS\n".to_owned();
            for device in devices {
                text.push_str(&format!(
                    "{}\t{}\t{}\t{}\t{}\n",
                    device.name,
                    device.kind,
                    device.model,
                    device_ips(device),
                    links_of(&info, &device.name).len()
                ));
            }
            Ok(Some(text.into_bytes()))
        }
        Command::Show {
            input,
            device,
            json,
        } => {
            let info = load_info(&input, true, true)?;
            let found = find_device(&info, &device).map_err(CliError::ordinary)?;
            if json {
                let value =
                    serde_json::json!({ "device": found, "links": links_of(&info, &found.name) });
                Ok(Some(
                    format!(
                        "{}\n",
                        serde_json::to_string_pretty(&value).map_err(CliError::ordinary)?
                    )
                    .into_bytes(),
                ))
            } else {
                Ok(Some(crate::device_markdown(&info, found).into_bytes()))
            }
        }
        Command::Links {
            input,
            device,
            json,
        } => {
            let info = load_info(&input, true, false)?;
            let links = if let Some(query) = device {
                let found = find_device(&info, &query).map_err(CliError::ordinary)?;
                links_of(&info, &found.name)
            } else {
                info.links.iter().collect()
            };
            if json {
                return Ok(Some(
                    format!(
                        "{}\n",
                        serde_json::to_string_pretty(&links).map_err(CliError::ordinary)?
                    )
                    .into_bytes(),
                ));
            }
            Ok(Some(
                links
                    .iter()
                    .map(|link| {
                        format!(
                            "{}:{} <-> {}:{} ({})\n",
                            link.a, link.a_port, link.b, link.b_port, link.cable
                        )
                    })
                    .collect::<String>()
                    .into_bytes(),
            ))
        }
        Command::Grep {
            input,
            regex,
            ignore_case,
            device,
        } => {
            let info = load_info(&input, true, true)?;
            let pattern = RegexBuilder::new(&regex)
                .case_insensitive(ignore_case)
                .build()
                .map_err(CliError::ordinary)?;
            let selected: Vec<_> = if let Some(query) = device {
                vec![find_device(&info, &query).map_err(CliError::ordinary)?]
            } else {
                info.devices.iter().collect()
            };
            let mut text = String::new();
            for device in selected {
                for line in device.running_config.iter().flatten() {
                    if pattern.is_match(line) {
                        text.push_str(&format!("{}: {}\n", device.name, line));
                    }
                }
            }
            if text.is_empty() {
                Err(CliError::grep_empty())
            } else {
                Ok(Some(text.into_bytes()))
            }
        }
        Command::Export {
            input,
            format,
            output,
            force,
        } => {
            let info = load_info(&input, true, false)?;
            let graph = Graph::new(&info);
            if matches!(format, ExportFormat::Obsidian) {
                let notes = crate::export::obsidian(&info);
                if output.exists() && !output.is_dir() {
                    return Err(CliError::ordinary(format!(
                        "{} da ton tai va khong phai thu muc",
                        output.display()
                    )));
                }
                if !force
                    && let Some((name, _)) =
                        notes.iter().find(|(name, _)| output.join(name).exists())
                {
                    return Err(CliError::ordinary(format!(
                        "{} da ton tai; dung --force de ghi de",
                        output.join(name).display()
                    )));
                }
                fs::create_dir_all(&output).map_err(CliError::ordinary)?;
                for (name, contents) in notes {
                    fs::write(output.join(name), contents).map_err(CliError::ordinary)?;
                }
                return Ok(Some(
                    format!("[+] {} -> {}\n", input.display(), output.display()).into_bytes(),
                ));
            }
            if output.exists() && !force {
                return Err(CliError::ordinary(format!(
                    "{} da ton tai; dung --force de ghi de",
                    output.display()
                )));
            }
            let text = match format {
                ExportFormat::Dot => crate::export::dot(&graph),
                ExportFormat::Mermaid => crate::export::mermaid(&graph),
                ExportFormat::Graphml => crate::export::graphml(&graph),
                ExportFormat::Svg => crate::export::svg(
                    &graph,
                    &force_directed(&graph, 0x504b_5432_4149, 120),
                    1000,
                    700,
                ),
                ExportFormat::Obsidian => unreachable!(),
            };
            fs::write(&output, text).map_err(CliError::ordinary)?;
            Ok(Some(
                format!("[+] {} -> {}\n", input.display(), output.display()).into_bytes(),
            ))
        }
    }
}

pub fn run_from<I, T>(arguments: I) -> u8
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = match Cli::try_parse_from(arguments) {
        Ok(cli) => cli,
        Err(error) => {
            let code = if error.use_stderr() { 2 } else { 0 };
            let _ = error.print();
            return code;
        }
    };
    match execute(cli.command) {
        Ok(Some(bytes)) => match io::stdout().lock().write_all(&bytes) {
            Ok(()) => 0,
            Err(error) if error.kind() == io::ErrorKind::BrokenPipe => 0,
            Err(error) => {
                let _ = writeln!(io::stderr().lock(), "[-] Loi: {error}");
                1
            }
        },
        Ok(None) => 0,
        Err(error) => {
            if !error.message.is_empty() {
                let _ = writeln!(io::stderr().lock(), "[-] Loi: {}", error.message);
            }
            error.code
        }
    }
}

pub fn run() -> u8 {
    run_from(std::env::args_os())
}
