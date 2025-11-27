use actix_cors::Cors;
use actix_files as fs;
use actix_web::body::MessageBody;
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::http::header::ContentType;
use actix_web::middleware::{from_fn, Next};
use actix_web::web::Query;
use actix_web::Result;
use actix_web::{error, Error};
use actix_web::{get, post, rt, web, HttpRequest, HttpResponse, Responder};
use actix_web::{App, HttpServer};
use actix_ws::AggregatedMessage;
use anyhow::Result as AnyResult;
use futures::StreamExt;
use std::collections::HashMap;
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

/// Need to authenticate if not login path
///
/// How to get token:
/// * From header `x-pisugar-token: <token>`
/// * From query parameter: `?token=<token>`
async fn token_auth_middleware(
    query: Query<HashMap<String, String>>,
    req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    let app_state = req.app_data::<web::Data<AppState>>().unwrap();
    let need_auth = app_state.core.lock().await.config().need_auth();

    if need_auth {
        log::info!("Authenticating request to {}", req.path());
        // ?token=<token>
        let token = query.get("token").map(|s| s.as_str());
        // x-pisugar-token: <token>
        let token = token.or_else(|| req.headers().get("x-pisugar-token").and_then(|v| v.to_str().ok()));
        // Verify JWT
        if token.is_none() || !jwt::verify_jwt(token.unwrap(), &app_state.jwt_secret).unwrap_or(false) {
            return Err(error::ErrorUnauthorized("Unauthorized"));
        }
    }

    next.call(req).await
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
struct ExecParams {
    cmd: Option<String>,
}

/// Execute a command, from query parameter or raw request body
#[post("")]
async fn exec(params: web::Query<ExecParams>, body: web::Bytes, app_state: web::Data<AppState>) -> impl Responder {
    let cmd: String = params
        .cmd
        .clone()
        .or_else(|| Some(String::from_utf8_lossy(&body).to_string()))
        .unwrap_or_default();
    let resp = cmds::handle_request(app_state.core.clone(), &cmd).await;
    if resp.is_ok() {
        HttpResponse::Ok()
            .content_type(ContentType::plaintext())
            .body(format!("{}", resp.result_string()))
    } else {
        HttpResponse::BadRequest()
            .content_type(ContentType::plaintext())
            .body(format!("{}", resp.result_string()))
    }
}

/// WebSocket endpoint
#[get("")]
pub async fn ws(req: HttpRequest, stream: web::Payload, app_state: web::Data<AppState>) -> Result<impl Responder> {
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
                        if line.is_empty() {
                            continue;
                        }
                        log::debug!("WS Received text: {}", line);
                        let resp = cmds::handle_request(core.clone(), line).await;
                        session_cloned
                            .lock()
                            .await
                            .text(format!("{resp}"))
                            .await
                            .expect("Failed to send response");
                    }
                }
                Ok(AggregatedMessage::Ping(msg)) => {
                    session_cloned
                        .lock()
                        .await
                        .pong(&msg)
                        .await
                        .expect("Http ws pong failed");
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
            session_cloned
                .lock()
                .await
                .text(s)
                .await
                .expect("Failed to send event via WS");
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
    debug: bool,
) {
    tokio::spawn(async move {
        if let Err(e) = build_run_server(
            core.clone(),
            event_rx.clone(),
            http_addr.clone(),
            web_dir.clone(),
            jwt_secret.clone(),
            debug,
        )
        .await
        {
            log::warn!("HTTP server error: {}", e);
        }
        log::info!("HTTP server on {} exited", http_addr);
    });
}

async fn build_run_server(
    core: Arc<Mutex<PiSugarCore>>,
    event_rx: Receiver<String>,
    http_addr: String,
    web_dir: String,
    jwt_secret: String,
    debug: bool,
) -> AnyResult<()> {
    let app_state = AppState {
        core,
        jwt_secret,
        event_rx,
    };

    let server = HttpServer::new(move || {
        let app = App::new().app_data(web::Data::new(app_state.clone()));
        let cors = if debug { Cors::permissive() } else { Cors::default() };
        app.wrap(cors)
            .service(login)
            .service(web::scope("/ws").wrap(from_fn(token_auth_middleware)).service(ws))
            .service(web::scope("/exec").wrap(from_fn(token_auth_middleware)).service(exec))
            .service(
                fs::Files::new("/", web_dir.clone())
                    .index_file("index.html")
                    .show_files_listing(),
            )
    })
    .shutdown_timeout(1)
    .bind(http_addr)?;
    server.disable_signals().run().await?;
    Ok(())
}
