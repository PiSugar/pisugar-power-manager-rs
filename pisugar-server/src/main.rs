use std::time::Duration;
use std::time::Instant;

use actix::prelude::*;
use actix_web::{App, Error, get, HttpRequest, HttpResponse, HttpServer, middleware, Responder, web};
use actix_web_actors::ws;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().body("Hello world!")
}

//#[get("/model")]
//async fn model() -> impl Responder {
//    unimplemented!()
//}
//
//#[get("/battery")]
//async fn battery() -> impl Responder {
//    unimplemented!()
//}
//
//#[get("/battery_v")]
//async fn battery_v() -> impl Responder {
//    unimplemented!()
//}
//
//#[get("/battery_i")]
//async fn battery_i() -> impl Responder {
//    unimplemented!()
//}
//
//#[get("/battery_charging")]
//async fn battery_charging() -> impl Responder {
//    unimplemented!()
//}
//
//#[get("/rtc_time")]
//async fn rtc_time() -> impl Responder {
//    unimplemented!()
//}
//
//#[get("/rtc_time_list")]
//async fn rtc_time_list() -> impl Responder {
//    unimplemented!()
//}
//
//#[get("/rtc_alarm_flag")]
//async fn rtc_alarm_flag() -> impl Responder {
//    unimplemented!()
//}


/// start websocket, to push events
async fn start_ws(r: HttpRequest, stream: web::Payload, data: web::Data<Addr<ServerMonitor>>) -> Result<HttpResponse, Error> {
    log::debug!("{:?}", r);

    let (addr, res) = ws::start_with_addr(MyWebSocket::new(), &r, stream)?;
    data.get_ref().do_send(RegisterWSClient { addr });

    Ok(res)
}

/// Client must send ping every 10s, otherwise will be dropped.
struct MyWebSocket {
    last_received: Instant
}

impl Actor for MyWebSocket {
    type Context = ws::WebsocketContext<Self>;

    /// Method is called on actor start. We start the heartbeat process here.
    fn started(&mut self, ctx: &mut Self::Context) {
        self.ping(ctx);
    }
}

/// Handler for `ws::Message`
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for MyWebSocket {
    fn handle(
        &mut self,
        msg: Result<ws::Message, ws::ProtocolError>,
        ctx: &mut Self::Context,
    ) {
        // process websocket messages
        log::info!("WS: {:?}", msg);
        self.last_received = Instant::now();
        match msg {
            Ok(ws::Message::Ping(msg)) => ctx.pong(&msg),
            Ok(ws::Message::Pong(_)) => {}
            Ok(ws::Message::Text(text)) => ctx.text(text),
            Ok(ws::Message::Binary(bin)) => ctx.binary(bin),
            Ok(ws::Message::Close(_)) => ctx.stop(),
            _ => ctx.stop(),
        }
    }
}

impl Handler<ServerEvent> for MyWebSocket {
    type Result = ();

    fn handle(&mut self, msg: ServerEvent, ctx: &mut Self::Context) {
        ctx.text(msg.event);
    }
}

impl MyWebSocket {
    fn new() -> Self {
        Self { last_received: Instant::now() }
    }

    /// helper method that sends ping to client every second.
    ///
    /// also this method checks heartbeats from client
    fn ping(&self, ctx: &mut <Self as Actor>::Context) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            // check client heartbeats
            if Instant::now().duration_since(act.last_received) > CLIENT_TIMEOUT {
                // heartbeat timed out
                println!("Websocket Client heartbeat failed, disconnecting!");

                // stop actor
                ctx.stop();

                // don't try to send a ping
                return;
            }

            ctx.ping(b"");
        });
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct RegisterWSClient {
    addr: Addr<MyWebSocket>,
}

#[derive(Message)]
#[rtype(result = "()")]
struct ServerEvent {
    event: String,
}

struct ServerMonitor {
    listeners: Vec<Addr<MyWebSocket>>,
}

impl Actor for ServerMonitor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.run_interval(Duration::from_secs(5), |act, _| {
            for l in &act.listeners {
                l.do_send(ServerEvent { event: String::from("Event:") });
            }
        });
    }
}

impl Handler<RegisterWSClient> for ServerMonitor {
    type Result = ();

    fn handle(&mut self, msg: RegisterWSClient, _: &mut Context<Self>) {
        self.listeners.push(msg.addr);
    }
}

pub struct PiSugarStatus {
    pub model: String;
    pub voltage: f64;
    pub intensity: f64;
    pub charging: f64;
}

pub struct PiSugarActor;

impl Actor for PiSugarActor {
    type Context = Context<PiSugarStatus>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::info!("pisugar power manager started");
        ctx.run_interval(Duration::from_millis(500), |actor, ctx| {
            log::debug!("polling pisugar status");
        });
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        log::info!("pisugar power manager stopped");
    }
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let sys = System::new("PiSugar Power Manager");

    let srvmon = ServerMonitor { listeners: vec![] }.start();

    HttpServer::new(move || {
        App::new().data(srvmon.clone())
            .wrap(middleware::Logger::default())
            .service(index)
            .service(web::resource("/ws/").to(start_ws))
    })
        .bind("[::0]:8080")?
        .run()
        .await
}
