use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;

use anyhow::Result;
use tokio::net::UnixStream;
use tokio::sync::watch::Receiver;
use tokio::sync::Mutex;

use pisugar_core::PiSugarCore;

use crate::stream::{handle_stream, handle_stream_strict};

/// Handle uds
async fn handle_uds_stream(
    core: Arc<Mutex<PiSugarCore>>,
    stream: UnixStream,
    event_rx: Receiver<String>,
    strict: bool,
) -> Result<()> {
    log::info!("Incoming uds stream: {:?}", stream.peer_addr()?);
    if strict {
        handle_stream_strict(core, stream, event_rx).await
    } else {
        handle_stream(core, stream, event_rx).await
    }
}

/// Start UDS server with a new async task
pub async fn start_uds_server(
    uds_addr: String,
    core: Arc<Mutex<PiSugarCore>>,
    event_rx: Receiver<String>,
    uds_mode: u32,
    strict: bool,
) {
    let core_cloned = core.clone();
    let event_rx_cloned = event_rx.clone();
    tokio::spawn(async move {
        match tokio::net::UnixListener::bind(&uds_addr) {
            Ok(uds_listener) => {
                log::info!("UDS listening...");
                let perm = fs::Permissions::from_mode(uds_mode);
                if let Err(e) = fs::set_permissions(&uds_addr, perm) {
                    log::warn!("Set uds file permission {} error: {}", uds_mode, e);
                }
                while let Ok((stream, addr)) = uds_listener.accept().await {
                    log::info!("UDS from {:?}", addr);
                    let core = core_cloned.clone();
                    if let Err(e) = handle_uds_stream(core, stream, event_rx_cloned.clone(), strict).await {
                        log::error!("Handle uds error: {}", e);
                    }
                }
                log::info!("UDS stopped");
            }
            Err(e) => {
                log::warn!("UDS bind error: {}", e);
            }
        }
    });
}
