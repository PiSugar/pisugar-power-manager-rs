use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use pisugar_core::PiSugarCore;
use tokio::net::TcpStream;

use anyhow::Result;
use tokio::sync::watch::Receiver;

use crate::stream::handle_stream;

/// Handle tcp stream
async fn handle_tcp_stream(core: Arc<Mutex<PiSugarCore>>, stream: TcpStream, event_rx: Receiver<String>) -> Result<()> {
    log::info!("Incoming tcp connection from: {}", stream.peer_addr()?);
    handle_stream(core, stream, event_rx).await
}

/// Start TCP server with a new async task
pub async fn start_tcp_server(core: Arc<Mutex<PiSugarCore>>, event_rx: Receiver<String>, tcp_addr: String) {
    tokio::spawn(async move {
        loop {
            match TcpListener::bind(&tcp_addr).await {
                Ok(tcp_listener) => {
                    log::info!("TCP listening...");
                    while let Ok((stream, addr)) = tcp_listener.accept().await {
                        log::info!("TCP from {}", addr);
                        if let Err(e) = handle_tcp_stream(core.clone(), stream, event_rx.clone()).await {
                            log::error!("Handle tcp error: {}", e);
                        }
                    }
                    log::info!("TCP stopped");
                }
                Err(e) => {
                    log::warn!("TCP bind error: {}", e);
                }
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    });
}
