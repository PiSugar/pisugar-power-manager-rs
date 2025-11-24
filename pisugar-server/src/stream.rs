use std::sync::Arc;

use anyhow::Result;
use futures::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::watch::Receiver;
use tokio::sync::Mutex;
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
    let (mut reader, writer) = tokio::io::split(stream);
    let writer = Arc::new(Mutex::new(writer));

    // handle request
    let writer_cloned = writer.clone();
    tokio::spawn(async move {
        let mut buf = [0; 4096];
        while let Ok(n) = reader.read(&mut buf[..]).await {
            let reqs = String::from_utf8_lossy(&buf[..n]).to_string();
            for req in reqs.lines() {
                if req.is_empty() {
                    continue;
                }
                log::debug!("Req: {}", req);
                let mut resp = cmds::handle_request(core.clone(), req).await.to_string();
                log::debug!("Resp: {}", resp);
                if !resp.ends_with("\n") {
                    resp.push('\n');
                }
                writer_cloned
                    .lock()
                    .await
                    .write_all(resp.as_bytes())
                    .await
                    .expect("Stream send error");
            }
        }
    });

    // button event
    let writer_cloned = writer.clone();
    tokio::spawn(async move {
        let _ = event_rx.borrow_and_update();
        while event_rx.changed().await.is_ok() {
            let mut s = event_rx.borrow().clone();
            if !s.ends_with("\n") {
                s.push('\n');
            }
            writer_cloned
                .lock()
                .await
                .write_all(s.as_bytes())
                .await
                .expect("Stream send error");
        }
    });

    Ok(())
}
