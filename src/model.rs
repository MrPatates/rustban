use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AppConfig {
    pub sends: Vec<VbanSend>,
    pub recvs: Vec<VbanRecv>,
    pub host_info_emulation: HostInfoEmulation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HostInfoEmulation {
    pub enabled: bool,
    pub app_name: String,
    pub host_name: String,
    pub user_name: String,
    pub client_name: String,
}

impl Default for HostInfoEmulation {
    fn default() -> Self {
        Self {
            enabled: false,
            app_name: "VBAN".into(),
            host_name: "vban-host".into(),
            user_name: "vban".into(),
            client_name: "VBAN Remote".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VbanSend {
    pub id: Uuid,
    pub enabled: bool,
    pub always_process: bool,
    pub destination_ip: String,
    pub destination_port: u16,
    pub sess_name: String,
    pub sess_media: String,
    pub audio_format: String,
    pub audio_rate: u32,
    pub audio_channels: u8,
    pub node_name: String,
    pub node_description: String,
    pub target_object: String,
}

impl Default for VbanSend {
    fn default() -> Self {
        let id = Uuid::new_v4();
        Self {
            id,
            enabled: true,
            always_process: false,
            destination_ip: "127.0.0.1".into(),
            destination_port: 6980,
            sess_name: "PipeWire VBAN stream".into(),
            sess_media: "audio".into(),
            audio_format: "S16LE".into(),
            audio_rate: 48_000,
            audio_channels: 2,
            node_name: format!("vban-send-{}", id.simple()),
            node_description: "VBAN Send".into(),
            target_object: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VbanRecv {
    pub id: Uuid,
    pub enabled: bool,
    pub source_ip: String,
    pub source_port: u16,
    pub latency_msec: u32,
    pub always_process: bool,
    pub stream_name: String,
    pub node_name: String,
    pub node_description: String,
}

impl Default for VbanRecv {
    fn default() -> Self {
        let id = Uuid::new_v4();
        Self {
            id,
            enabled: true,
            source_ip: "127.0.0.1".into(),
            source_port: 6980,
            latency_msec: 100,
            always_process: false,
            stream_name: String::new(),
            node_name: format!("vban-recv-{}", id.simple()),
            node_description: "VBAN Recv".into(),
        }
    }
}
