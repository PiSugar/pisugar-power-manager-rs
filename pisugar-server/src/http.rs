use actix_files as fs;
use actix_web::http::header::ContentType;
use actix_web::Result;
use actix_web::{get, post, rt, web, HttpRequest, HttpResponse, Responder};
use actix_web::{App, HttpServer};
use actix_ws::AggregatedMessage;
use anyhow::Result as AnyResult;
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::watch::Receiver;
use tokio::sync::Mutex;

use pisugar_core::PiSugarCore;

use crate::cmds;
use crate::jwt;

#[derive(Clone)]
struct AppState {
    jwt_secret: String,
    core: Arc<Mutex<PiSugarCore>>,
    event_rx: Receiver<String>,
}

#[derive(serde::Deserialize)]
struct LoginParams {
    username: Option<String>,
    password: Option<String>,
}

/// Login to get a JWT token
#[post("/login")]
async fn login(params: web::Query<LoginParams>, app_state: web::Data<AppState>) -> impl Responder {
    let core = app_state.core.lock().await;
    let cfg = core.config();
    let mut auth_ok = !cfg.need_auth();
    if cfg.need_auth() && params.username.is_some() && params.password.is_some() {
        let auth_user = cfg.auth_user.as_ref();
        let auth_pass = cfg.auth_password.as_ref();
        if params.username.as_ref() == auth_user && params.password.as_ref() == auth_pass {
            auth_ok = true;
        }
    }
    let username = params.username.clone().unwrap_or_default();
    if auth_ok {
        if let Ok(token) = jwt::generate_jwt(&username, &app_state.jwt_secret, cfg.session_timeout as u64) {
            return HttpResponse::Ok().content_type(ContentType::plaintext()).body(token);
        }
    }
    HttpResponse::Unauthorized().finish()
}

#[derive(serde::Deserialize)]
struct WSParams {
    token: Option<String>,
}

/// WebSocket endpoint
#[get("/ws")]
pub async fn ws(
    params: web::Query<WSParams>,
    req: HttpRequest,
    stream: web::Payload,
    app_state: web::Data<AppState>,
) -> Result<impl Responder> {
    let core = app_state.core.lock().await;
    // Verify JWT
    if core.config().need_auth() {
        let token = params.into_inner().token;
        if token.is_none() || jwt::verify_jwt(&token.unwrap(), &app_state.jwt_secret).is_err() {
            return Ok(HttpResponse::Unauthorized().finish());
        }
    }

    let (res, session, stream) = actix_ws::handle(&req, stream)?;
    let mut stream = stream.aggregate_continuations().max_continuation_size(1024 * 1024);
    let session = Arc::new(Mutex::new(session));

    // Read request
    let session_cloned = session.clone();
    let core = app_state.core.clone();
    rt::spawn(async move {
        while let Some(msg) = stream.next().await {
            match msg {
                Ok(AggregatedMessage::Text(text)) => {
                    for line in text.split("\n") {
                        log::debug!("WS Received text: {}", line);
                        let mut resp = cmds::handle_request(core.clone(), line).await;
                        if !resp.ends_with('\n') {
                            resp.push('\n');
                        }
                        if let Err(e) = session_cloned.lock().await.text(resp).await {
                            log::error!("Failed to send response via WS: {}", e);
                            return;
                        }
                    }
                }
                Ok(AggregatedMessage::Ping(msg)) => {
                    session_cloned.lock().await.pong(&msg).await.expect("");
                }
                _ => {}
            };
        }
    });

    // Write events
    let mut event_rx = app_state.event_rx.clone();
    let session_cloned = session.clone();
    rt::spawn(async move {
        while event_rx.changed().await.is_ok() {
            let s = event_rx.borrow().clone();
            log::debug!("WS Sending event: {}", s);
            if let Err(e) = session_cloned.lock().await.text(s).await {
                log::error!("Failed to send event via WS: {}", e);
                break;
            }
        }
    });

    Ok(res)
}

/// Start the HTTP server with a new async task
pub async fn start_http_server(
    core: Arc<Mutex<PiSugarCore>>,
    event_rx: Receiver<String>,
    http_addr: String,
    web_dir: String,
    jwt_secret: String,
) {
    tokio::spawn(async move {
        loop {
            if let Err(e) = build_run_server(
                core.clone(),
                event_rx.clone(),
                http_addr.clone(),
                web_dir.clone(),
                jwt_secret.clone(),
            )
            .await
            {
                log::warn!("HTTP server error: {}", e);
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        }
    });
}

async fn build_run_server(
    core: Arc<Mutex<PiSugarCore>>,
    event_rx: Receiver<String>,
    http_addr: String,
    web_dir: String,
    jwt_secret: String,
) -> AnyResult<()> {
    let app_state = AppState {
        core,
        jwt_secret,
        event_rx,
    };

    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(app_state.clone()))
            .service(login)
            .service(ws)
            .service(
                fs::Files::new("/", web_dir.clone())
                    .index_file("index.html")
                    .show_files_listing(),
            )
    })
    .shutdown_timeout(1)
    .bind(http_addr)?;
    server.run().await?;
    Ok(())
}
