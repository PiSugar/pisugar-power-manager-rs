use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use futures::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::watch::Receiver;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};

use pisugar_core::PiSugarCore;

use crate::cmds;

/// Handle a stream with '\n' as the flag
pub async fn handle_stream_strict<T>(
    core: Arc<Mutex<PiSugarCore>>,
    stream: T,
    mut event_rx: Receiver<String>,
) -> Result<()>
where
    T: 'static + AsyncRead + AsyncWrite + Send,
{
    let (reader, writer) = tokio::io::split(stream);
    let mut reader = FramedRead::new(reader, LinesCodec::new());
    let writer = Arc::new(Mutex::new(FramedWrite::new(writer, LinesCodec::new())));

    // handle request
    let writer_cloned = writer.clone();
    tokio::spawn(async move {
        while let Some(Ok(req)) = reader.next().await {
            log::debug!("Req: {}", req);
            let resp = cmds::handle_request(core.clone(), &req).await;
            log::debug!("Resp: {}", resp);
            if let Err(e) = (writer_cloned.lock().await).send(resp.to_string()).await {
                log::warn!("Stream send error: {}", e);
                return;
            }
        }
    });

    // button event
    let writer_cloned = writer.clone();
    tokio::spawn(async move {
        let _ = event_rx.borrow_and_update();
        while event_rx.changed().await.is_ok() {
            let s = event_rx.borrow().clone();
            if let Err(e) = (writer_cloned.lock().await).send(s).await {
                log::warn!("Stream send error: {}", e);
                break;
            }
        }
    });

    Ok(())
}

/// Handle a stream with or without '\n' as the flag
pub async fn handle_stream<T>(core: Arc<Mutex<PiSugarCore>>, stream: T, mut event_rx: Receiver<String>) -> Result<()>
where
    T: 'static + AsyncRead + AsyncWrite + Send,
{
    let (mut reader, mut writer) = tokio::io::split(stream);
    let (resp_tx, mut resp_rx) = tokio::sync::mpsc::unbounded_channel::<Result<String>>();

    // handle request
    let resp_tx_cloned = resp_tx.clone();
    tokio::spawn(async move {
        let resp_tx = resp_tx_cloned;
        let mut buf = [0; 4096];

        'session_loop: loop {
            if let Ok(n) = reader.read(&mut buf[..]).await {
                if n == 0 {
                    break;
                }
                let reqs = String::from_utf8_lossy(&buf[..n]).to_string();
                for req in reqs.lines() {
                    if req.is_empty() {
                        continue;
                    }
                    log::debug!("Req: {}", req);
                    let resp = cmds::handle_request(core.clone(), req).await.to_string();
                    log::debug!("Resp: {}", resp);
                    if let Err(e) = resp_tx.send(Ok(resp)) {
                        log::debug!("Response channel error: {}", e);
                        break 'session_loop;
                    }
                }
            }
        }
        let _ = resp_tx.send(Err(anyhow!("Stream closed")));
    });

    // button event
    tokio::spawn(async move {
        let _ = event_rx.borrow_and_update();

        loop {
            match timeout(Duration::from_secs(1), event_rx.changed()).await {
                Ok(Ok(())) => {
                    let s = event_rx.borrow().clone();
                    if let Err(e) = resp_tx.send(Ok(s)) {
                        log::debug!("Response channel error: {}", e);
                        break;
                    }
                }
                Ok(Err(_)) => {
                    log::debug!("Event channel closed");
                    break;
                }
                Err(_) => {} // timeout
            }
            if resp_tx.is_closed() {
                log::debug!("Response channel closed");
                break;
            }
        }
    });

    // Output
    tokio::spawn(async move {
        while let Some(resp) = resp_rx.recv().await {
            match resp {
                Ok(s) => {
                    let mut s = s;
                    if !s.ends_with("\n") {
                        s.push('\n');
                    }
                    if let Err(e) = writer.write_all(s.as_bytes()).await {
                        log::warn!("Stream send error: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    log::warn!("Response error: {}", e);
                    break;
                }
            }
        }
        let _ = writer.shutdown().await;
    });

    Ok(())
}
