use anyhow::{Context, Result};
use directories::BaseDirs;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::{
    model::AppConfig,
    pipewire_conf::{filename_recv, filename_send, render_recv, render_send},
};

#[derive(Debug, Clone)]
pub struct AudioSourceDevice {
    pub node_name: String,
    pub description: String,
}

#[derive(Debug, Clone, Default)]
pub struct AutoLinkSummary {
    pub links_created: usize,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone)]
struct PipewirePort {
    port_name: String,
    is_input: bool,
    channel: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct PipewireTopology {
    nodes_by_name: HashMap<String, u32>,
    ports_by_node: HashMap<u32, Vec<PipewirePort>>,
}

pub fn config_dir() -> Result<PathBuf> {
    let base = BaseDirs::new().context("Cannot detect HOME")?;
    Ok(base.config_dir().join("rustban"))
}

pub fn pipewire_dropin_dir() -> Result<PathBuf> {
    let base = BaseDirs::new().context("Cannot detect HOME")?;
    Ok(base.config_dir().join("pipewire").join("pipewire.conf.d"))
}

pub fn load_app_config() -> Result<AppConfig> {
    let dir = config_dir()?;
    fs::create_dir_all(&dir)?;
    let path = dir.join("config.toml");
    if !path.exists() {
        let cfg = AppConfig::default();
        save_app_config(&cfg)?;
        return Ok(cfg);
    }

    let raw = fs::read_to_string(&path)?;
    let cfg: AppConfig = toml::from_str(&raw)?;
    Ok(cfg)
}

pub fn save_app_config(cfg: &AppConfig) -> Result<()> {
    let dir = config_dir()?;
    fs::create_dir_all(&dir)?;
    let path = dir.join("config.toml");
    let raw = toml::to_string_pretty(cfg)?;
    fs::write(path, raw)?;
    Ok(())
}

pub fn apply_pipewire_fragments(cfg: &AppConfig) -> Result<()> {
    let dir = pipewire_dropin_dir()?;
    fs::create_dir_all(&dir)?;

    let mut keep: HashSet<String> = HashSet::new();

    for send in &cfg.sends {
        let id = send.id.simple().to_string();
        let file_name = filename_send(&id);
        keep.insert(file_name.clone());
        let path = dir.join(&file_name);

        if send.enabled {
            fs::write(path, render_send(send, &cfg.host_info_emulation))?;
        } else if path.exists() {
            fs::remove_file(path)?;
        }
    }

    for recv in &cfg.recvs {
        let id = recv.id.simple().to_string();
        let file_name = filename_recv(&id);
        keep.insert(file_name.clone());
        let path = dir.join(&file_name);

        if recv.enabled {
            fs::write(path, render_recv(recv, &cfg.host_info_emulation))?;
        } else if path.exists() {
            fs::remove_file(path)?;
        }
    }

    cleanup_removed_entries(&dir, &keep)?;
    Ok(())
}

pub fn autolink_send_sources(cfg: &AppConfig) -> Result<AutoLinkSummary> {
    let sends_to_link: Vec<_> = cfg
        .sends
        .iter()
        .filter(|send| send.enabled && !send.target_object.trim().is_empty())
        .collect();
    if sends_to_link.is_empty() {
        return Ok(AutoLinkSummary::default());
    }

    let topology = load_pipewire_topology()?;
    let mut summary = AutoLinkSummary::default();

    for send in sends_to_link {
        let source_node_name = send.target_object.trim();
        let send_node_name = send.node_name.trim();

        let Some(&source_node_id) = topology.nodes_by_name.get(source_node_name) else {
            summary.issues.push(format!(
                "Source `{source_node_name}` not found in PipeWire."
            ));
            continue;
        };
        let Some(&send_node_id) = topology.nodes_by_name.get(send_node_name) else {
            summary.issues.push(format!(
                "Send node `{send_node_name}` not found (try `Apply + restart`)."
            ));
            continue;
        };

        let source_ports: Vec<_> = topology
            .ports_by_node
            .get(&source_node_id)
            .map(|ports| ports.iter().filter(|port| !port.is_input).collect())
            .unwrap_or_default();
        let send_ports: Vec<_> = topology
            .ports_by_node
            .get(&send_node_id)
            .map(|ports| ports.iter().filter(|port| port.is_input).collect())
            .unwrap_or_default();

        if source_ports.is_empty() {
            summary.issues.push(format!(
                "Source `{source_node_name}` has no output audio ports."
            ));
            continue;
        }
        if send_ports.is_empty() {
            summary
                .issues
                .push(format!("Send `{send_node_name}` has no input audio ports."));
            continue;
        }

        let planned_links = plan_autolinks(&source_ports, &send_ports);
        if planned_links.is_empty() {
            summary
                .issues
                .push(format!("No compatible ports found for `{send_node_name}`."));
            continue;
        }

        for (source_port, send_port) in planned_links {
            match ensure_pw_link(source_node_name, &source_port, send_node_name, &send_port) {
                Ok(true) => {
                    summary.links_created += 1;
                }
                Ok(false) => {}
                Err(e) => summary.issues.push(format!(
                    "{source_node_name}:{source_port} -> {send_node_name}:{send_port}: {e:#}"
                )),
            }
        }
    }

    Ok(summary)
}

fn cleanup_removed_entries(dir: &Path, keep: &HashSet<String>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };

        if !is_rustban_fragment(name) {
            continue;
        }

        if keep.contains(name) {
            continue;
        }

        let path = entry.path();
        if path.is_file() {
            fs::remove_file(path)?;
        }
    }

    Ok(())
}

fn is_rustban_fragment(name: &str) -> bool {
    let is_send = name.starts_with("99-rustban-send-");
    let is_recv = name.starts_with("99-rustban-recv-");
    (is_send || is_recv) && name.ends_with(".conf")
}

fn load_pipewire_topology() -> Result<PipewireTopology> {
    let mut topology = PipewireTopology::default();

    let nodes_output = Command::new("pw-dump")
        .arg("Node")
        .output()
        .context("Could not execute `pw-dump Node`")?;
    if !nodes_output.status.success() {
        anyhow::bail!("`pw-dump Node` exited with status {}", nodes_output.status);
    }

    let node_entries: Vec<Value> = serde_json::from_slice(&nodes_output.stdout)
        .context("Could not parse JSON output from `pw-dump Node`")?;
    for entry in node_entries {
        let Some(node_id) = entry.get("id").and_then(value_to_u32) else {
            continue;
        };
        let Some(node_name) = entry
            .get("info")
            .and_then(|info| info.get("props"))
            .and_then(|props| props.get("node.name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
        else {
            continue;
        };

        topology
            .nodes_by_name
            .insert(node_name.to_string(), node_id);
    }

    let ports_output = Command::new("pw-dump")
        .arg("Port")
        .output()
        .context("Could not execute `pw-dump Port`")?;
    if !ports_output.status.success() {
        anyhow::bail!("`pw-dump Port` exited with status {}", ports_output.status);
    }

    let port_entries: Vec<Value> = serde_json::from_slice(&ports_output.stdout)
        .context("Could not parse JSON output from `pw-dump Port`")?;
    for entry in port_entries {
        let Some(info) = entry.get("info") else {
            continue;
        };
        let Some(props) = info.get("props").and_then(Value::as_object) else {
            continue;
        };

        let Some(node_id) = props.get("node.id").and_then(value_to_u32) else {
            continue;
        };
        let Some(port_name) = props
            .get("port.name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
        else {
            continue;
        };
        let Some(direction) = info.get("direction").and_then(Value::as_str).map(str::trim) else {
            continue;
        };

        let is_input = match direction {
            "input" => true,
            "output" => false,
            _ => continue,
        };
        let channel = props
            .get("audio.channel")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|channel| !channel.is_empty())
            .map(ToOwned::to_owned);

        topology
            .ports_by_node
            .entry(node_id)
            .or_default()
            .push(PipewirePort {
                port_name: port_name.to_string(),
                is_input,
                channel,
            });
    }

    Ok(topology)
}

fn plan_autolinks(
    source_ports: &[&PipewirePort],
    send_ports: &[&PipewirePort],
) -> Vec<(String, String)> {
    let mut links = Vec::new();

    for send_port in send_ports {
        let Some(source_port) = pick_source_port_for_send(source_ports, send_port) else {
            continue;
        };
        links.push((source_port.port_name.clone(), send_port.port_name.clone()));
    }

    links.sort();
    links.dedup();
    links
}

fn pick_source_port_for_send<'a>(
    source_ports: &'a [&PipewirePort],
    send_port: &PipewirePort,
) -> Option<&'a PipewirePort> {
    let target_channel = send_port.channel.as_deref().unwrap_or_default();

    if !target_channel.is_empty() {
        if let Some(source_port) = source_ports
            .iter()
            .find(|port| channels_match(port.channel.as_deref(), Some(target_channel)))
        {
            return Some(*source_port);
        }

        if target_channel.eq_ignore_ascii_case("FL") || target_channel.eq_ignore_ascii_case("FR") {
            if let Some(source_port) = source_ports
                .iter()
                .find(|port| channels_match(port.channel.as_deref(), Some("MONO")))
            {
                return Some(*source_port);
            }
        }
    }

    source_ports.first().copied()
}

fn channels_match(left: Option<&str>, right: Option<&str>) -> bool {
    matches!((left, right), (Some(a), Some(b)) if a.eq_ignore_ascii_case(b))
}

fn ensure_pw_link(
    source_node_name: &str,
    source_port_name: &str,
    send_node_name: &str,
    send_port_name: &str,
) -> Result<bool> {
    let source = format!("{source_node_name}:{source_port_name}");
    let target = format!("{send_node_name}:{send_port_name}");
    let output = Command::new("pw-link")
        .args([source.as_str(), target.as_str()])
        .output()
        .with_context(|| format!("Could not execute `pw-link {source} {target}`"))?;

    if output.status.success() {
        return Ok(true);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr_lower = stderr.to_ascii_lowercase();
    if stderr_lower.contains("file exists")
        || stderr_lower.contains("already linked")
        || stderr_lower.contains("already exists")
    {
        return Ok(false);
    }

    anyhow::bail!("`pw-link` failed: {}", stderr.trim());
}

fn value_to_u32(value: &Value) -> Option<u32> {
    value
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .or_else(|| value.as_str()?.trim().parse().ok())
}

pub fn list_microphone_sources() -> Result<Vec<AudioSourceDevice>> {
    let pw_dump_result = list_microphone_sources_pw_dump();
    let pactl_result = list_microphone_sources_pactl();

    match (pw_dump_result, pactl_result) {
        (Ok(pw_dump_devices), Ok(pactl_devices)) => {
            let merged = merge_audio_sources(pw_dump_devices, pactl_devices);
            Ok(merged)
        }
        (Ok(pw_dump_devices), Err(_)) => Ok(pw_dump_devices),
        (Err(_), Ok(pactl_devices)) => Ok(pactl_devices),
        (Err(pw_dump_error), Err(pactl_error)) => anyhow::bail!(
            "Could not list PipeWire sources. pw-dump: {pw_dump_error:#} | pactl: {pactl_error:#}"
        ),
    }
}

fn list_microphone_sources_pw_dump() -> Result<Vec<AudioSourceDevice>> {
    let output = Command::new("pw-dump")
        .arg("Node")
        .output()
        .context("Could not execute `pw-dump Node`")?;
    if !output.status.success() {
        anyhow::bail!("`pw-dump Node` exited with status {}", output.status);
    }

    let entries: Vec<Value> = serde_json::from_slice(&output.stdout)
        .context("Could not parse JSON output from `pw-dump Node`")?;
    Ok(extract_audio_sources(entries.into_iter()))
}

fn list_microphone_sources_pactl() -> Result<Vec<AudioSourceDevice>> {
    let output = Command::new("pactl")
        .args(["-f", "json", "list", "sources"])
        .output()
        .context("Could not execute `pactl -f json list sources`")?;
    if !output.status.success() {
        anyhow::bail!(
            "`pactl -f json list sources` exited with status {}",
            output.status
        );
    }

    let entries: Vec<Value> = serde_json::from_slice(&output.stdout)
        .context("Could not parse JSON output from `pactl -f json list sources`")?;
    Ok(extract_audio_sources(entries.into_iter()))
}

fn extract_audio_sources(entries: impl Iterator<Item = Value>) -> Vec<AudioSourceDevice> {
    let mut seen_names = HashSet::new();
    let mut devices = Vec::new();

    for entry in entries {
        let is_pw_dump_entry = entry.get("info").is_some();
        let Some(props) = entry
            .get("info")
            .and_then(|info| info.get("props"))
            .and_then(|props| props.as_object())
            .or_else(|| entry.get("properties").and_then(Value::as_object))
        else {
            continue;
        };

        let media_class = props
            .get("media.class")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if is_pw_dump_entry && !is_audio_source_media_class(media_class) {
            continue;
        }

        let node_name = props
            .get("node.name")
            .or_else(|| props.get("source_name"))
            .or_else(|| props.get("name"))
            .or_else(|| entry.get("name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        if node_name.is_empty() || is_monitor_source(node_name) {
            continue;
        }

        let description = props
            .get("node.description")
            .or_else(|| props.get("device.description"))
            .or_else(|| props.get("description"))
            .or_else(|| entry.get("description"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|desc| !desc.is_empty())
            .unwrap_or(node_name);

        if seen_names.insert(node_name.to_string()) {
            devices.push(AudioSourceDevice {
                node_name: node_name.to_string(),
                description: description.to_string(),
            });
        }
    }

    devices.sort_by(|a, b| {
        let a_key = a.description.to_lowercase();
        let b_key = b.description.to_lowercase();
        a_key
            .cmp(&b_key)
            .then_with(|| a.node_name.cmp(&b.node_name))
    });

    devices
}

fn is_audio_source_media_class(media_class: &str) -> bool {
    media_class.eq_ignore_ascii_case("Audio/Source")
        || media_class
            .get(..13)
            .map(|prefix| prefix.eq_ignore_ascii_case("Audio/Source/"))
            .unwrap_or(false)
}

fn merge_audio_sources(
    first: Vec<AudioSourceDevice>,
    second: Vec<AudioSourceDevice>,
) -> Vec<AudioSourceDevice> {
    let mut seen_names = HashSet::new();
    let mut merged = Vec::new();

    for device in first.into_iter().chain(second) {
        if seen_names.insert(device.node_name.clone()) {
            merged.push(device);
        }
    }

    merged.sort_by(|a, b| {
        let a_key = a.description.to_lowercase();
        let b_key = b.description.to_lowercase();
        a_key
            .cmp(&b_key)
            .then_with(|| a.node_name.cmp(&b.node_name))
    });

    merged
}

fn is_monitor_source(node_name: &str) -> bool {
    node_name.contains(".monitor")
}

pub fn restart_pipewire_user_services() -> Result<()> {
    let candidates = [
        vec![
            "--user",
            "restart",
            "pipewire.service",
            "pipewire-pulse.service",
        ],
        vec!["--user", "restart", "pipewire.service"],
        vec!["--user", "restart", "pipewire"],
    ];

    for args in candidates {
        let ok = Command::new("systemctl")
            .args(args)
            .status()
            .map(|status| status.success())
            .unwrap_or(false);

        if ok {
            return Ok(());
        }
    }

    anyhow::bail!("Could not restart pipewire via systemctl --user")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn source_entry(node_name: &str, description: &str, media_class: &str) -> Value {
        json!({
            "info": {
                "props": {
                    "node.name": node_name,
                    "node.description": description,
                    "media.class": media_class
                }
            }
        })
    }

    #[test]
    fn includes_audio_source_virtual_from_pw_dump() {
        let entries = vec![source_entry(
            "easyeffects_source",
            "Easy Effects Source",
            "Audio/Source/Virtual",
        )];

        let devices = extract_audio_sources(entries.into_iter());
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].node_name, "easyeffects_source");
    }

    #[test]
    fn excludes_non_source_pw_dump_nodes() {
        let entries = vec![source_entry(
            "alsa_output.some_sink",
            "Some Sink",
            "Audio/Sink",
        )];

        let devices = extract_audio_sources(entries.into_iter());
        assert!(devices.is_empty());
    }
}
