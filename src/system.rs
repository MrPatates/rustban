use anyhow::{Context, Result};
use directories::BaseDirs;
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
