use actix_web::{get, App, HttpResponse, HttpServer, Responder};

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

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| App::new().service(index))
        .bind("[::0]:8080")?
        .run()
        .await
}
