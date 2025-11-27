use futures::{SinkExt, StreamExt};
use pisugar_core::PiSugarCore;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::watch::Receiver;
use tokio::sync::Mutex;

use anyhow::Result;

use crate::cmds;

/// Handle websocket request
async fn handle_ws_connection(
    core: Arc<Mutex<PiSugarCore>>,
    stream: TcpStream,
    mut event_rx: Receiver<String>,
) -> Result<()> {
    log::info!("Incoming ws connection from: {}", stream.peer_addr()?);

    let ws_stream = tokio_tungstenite::accept_async(stream).await?;
    log::info!("WS connection established");

    let (sink, mut stream) = ws_stream.split();
    let sink = Arc::new(Mutex::new(sink));

    // handle request
    let sink_cloned = sink.clone();
    tokio::spawn(async move {
        while let Some(Ok(msg)) = stream.next().await {
            if let Ok(msg) = msg.to_text() {
                for req in msg.lines() {
                    if req.is_empty() {
                        continue;
                    }
                    log::debug!("Req: {}", req);
                    let resp = cmds::handle_request(core.clone(), req).await;
                    log::debug!("Resp: {}", resp);
                    sink_cloned
                        .lock()
                        .await
                        .send(resp.to_string().into())
                        .await
                        .expect("WS send error");
                }
            }
        }
    });

    // button event
    let sink_cloned = sink.clone();
    tokio::spawn(async move {
        while event_rx.changed().await.is_ok() {
            let s = event_rx.borrow().clone();
            sink_cloned.lock().await.send(s.into()).await.expect("WS send error");
        }
    });

    Ok(())
}

pub async fn start_ws_server(core: Arc<Mutex<PiSugarCore>>, event_rx: Receiver<String>, ws_addr: String) {
    tokio::spawn(async move {
        match tokio::net::TcpListener::bind(&ws_addr).await {
            Ok(ws_listener) => {
                log::info!("WS listening...");
                while let Ok((stream, addr)) = ws_listener.accept().await {
                    log::info!("WS from {}", addr);
                    let core = core.clone();
                    if let Err(e) = handle_ws_connection(core, stream, event_rx.clone()).await {
                        log::warn!("Handle ws error: {}", e);
                    }
                }
                log::info!("WS stopped");
            }
            Err(e) => {
                log::warn!("WS bind error: {}", e);
            }
        }
    });
}
