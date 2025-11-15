use std::sync::Arc;

use anyhow::Result;
use futures::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::watch::Receiver;
use tokio::sync::Mutex;
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};

use pisugar_core::PiSugarCore;

use crate::cmds;

pub async fn handle_stream<T>(core: Arc<Mutex<PiSugarCore>>, stream: T, mut event_rx: Receiver<String>) -> Result<()>
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
            if let Err(e) = (writer_cloned.lock().await).send(resp).await {
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
