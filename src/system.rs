use anyhow::{Context, Result};
use directories::BaseDirs;
use serde_json::Value;
use std::{
    collections::HashSet,
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
            fs::write(path, render_send(send))?;
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
            fs::write(path, render_recv(recv))?;
        } else if path.exists() {
            fs::remove_file(path)?;
        }
    }

    cleanup_removed_entries(&dir, &keep)?;
    Ok(())
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

pub fn list_microphone_sources() -> Result<Vec<AudioSourceDevice>> {
    match list_microphone_sources_pw_dump() {
        Ok(devices) if !devices.is_empty() => Ok(devices),
        Ok(pw_dump_devices) => match list_microphone_sources_pactl() {
            Ok(pactl_devices) if !pactl_devices.is_empty() => Ok(pactl_devices),
            Ok(_) => Ok(pw_dump_devices),
            Err(_) => Ok(pw_dump_devices),
        },
        Err(pw_dump_error) => match list_microphone_sources_pactl() {
            Ok(pactl_devices) => Ok(pactl_devices),
            Err(pactl_error) => anyhow::bail!(
                "Could not list PipeWire sources. pw-dump: {pw_dump_error:#} | pactl: {pactl_error:#}"
            ),
        },
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
        if is_pw_dump_entry && media_class != "Audio/Source" {
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
