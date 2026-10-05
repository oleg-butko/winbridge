//! Configuration for one explicitly selected local Debian/Windows pair.
use crate::config::{read_config, valid_id, DeliverySettings};
use serde::Deserialize;
use std::{
    collections::HashSet,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
};

pub fn local_ip(ip: Ipv4Addr) -> bool {
    ip.is_private() || ip.is_loopback()
}
pub fn validate_controller_url(url: &str) -> Result<SocketAddr, String> {
    let address: SocketAddr = url.strip_prefix("ws://").and_then(|s| s.strip_suffix('/').or(Some(s)))
        .ok_or("worker.controllerUrl must use ws://")?
        .parse().map_err(|_| "worker.controllerUrl must be ws://LOCAL_IPV4:PORT, without credentials, query or extra path".to_string())?;
    match address.ip() {
        IpAddr::V4(ip) if local_ip(ip) && address.port() != 0 => Ok(address),
        _ => Err("worker.controllerUrl requires a literal private/loopback IPv4 address and nonzero port".into()),
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ControllerConfig {
    pub schema_version: u16,
    pub instance_id: String,
    pub node_id: String,
    pub state_directory: PathBuf,
    pub controller: ControllerSettings,
    pub delivery: Option<DeliverySettings>,
    #[serde(default)]
    pub projects: Vec<SourceProject>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ControllerSettings {
    pub listen_address: SocketAddr,
    pub allowed_worker_addresses: Vec<Ipv4Addr>,
    pub worker_node_id: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceProject {
    pub id: String,
    pub source_root: PathBuf,
    pub source_mode: String,
}
impl ControllerConfig {
    pub fn load(path: &Path) -> Result<Self, String> {
        let config: Self = read_config(path)?;
        config.validate()?;
        Ok(config)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("controller schemaVersion must be 1".into());
        }
        if !valid_id(&self.instance_id)
            || !valid_id(&self.node_id)
            || !valid_id(&self.controller.worker_node_id)
        {
            return Err("controller instanceId/nodeId/workerNodeId must use 1–80 letters, digits, '-' or '_'".into());
        }
        if !self.state_directory.is_absolute() {
            return Err(
                "controller stateDirectory must be an absolute private local directory".into(),
            );
        }
        if !matches!(self.controller.listen_address.ip(), IpAddr::V4(ip) if local_ip(ip))
            || self.controller.listen_address.port() == 0
        {
            return Err("controller.listenAddress requires an explicit private/loopback IPv4 address and nonzero port; 0.0.0.0 is not allowed".into());
        }
        let peers = &self.controller.allowed_worker_addresses;
        if peers.is_empty() || peers.len() > 8 || peers.iter().any(|ip| !local_ip(*ip)) {
            return Err("controller.allowedWorkerAddresses must contain 1–8 literal private/loopback IPv4 addresses".into());
        }
        let mut ids = HashSet::new();
        for project in &self.projects {
            if !valid_id(&project.id)
                || !ids.insert(&project.id)
                || !project.source_root.is_absolute()
                || project.source_mode != "commit"
            {
                return Err("controller projects require unique IDs, absolute sourceRoot and sourceMode commit; import is not yet active".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn urls_reject_public_wildcard_tls_credentials_and_names() {
        for url in [
            "wss://192.168.1.1:1143",
            "ws://0.0.0.0:1143",
            "ws://8.8.8.8:1143",
            "ws://localhost:1143",
            "ws://u:p@192.168.1.1:1143",
            "ws://192.168.1.1:1143/path",
            "ws://192.168.1.1:0",
        ] {
            assert!(validate_controller_url(url).is_err(), "{url}");
        }
        assert!(validate_controller_url("ws://192.168.44.203:1143/").is_ok());
    }
    #[test]
    fn controller_requires_explicit_pair_and_schema() {
        let mut config: ControllerConfig = serde_json::from_value(serde_json::json!({
            "schemaVersion":1,"instanceId":"pair","nodeId":"debian","stateDirectory":std::env::temp_dir().join("private-controller"),
            "controller":{"listenAddress":"127.0.0.1:1143","allowedWorkerAddresses":["127.0.0.1"],"workerNodeId":"windows"}
        })).unwrap();
        config.validate().unwrap();
        config.controller.listen_address = "0.0.0.0:1143".parse().unwrap();
        assert!(config.validate().unwrap_err().contains("0.0.0.0"));
        config.controller.listen_address = "127.0.0.1:1143".parse().unwrap();
        config.controller.allowed_worker_addresses.clear();
        assert!(config.validate().is_err());
        config.schema_version = 99;
        assert!(config.validate().unwrap_err().contains("schemaVersion"));
    }
}
