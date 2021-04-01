use actix::prelude::*;
use actix_web::{get, middleware, web, App, Error, HttpRequest, HttpResponse, HttpServer};
use actix_web_actors::ws;
use actix_files as fs;
use log::{debug, info};

use telemetry_server::actors::*;
use telemetry_server::data::*;

#[get("/realtime/{full_key}")]
async fn realtime_index(
    r: HttpRequest,
    stream: web::Payload,
    info: web::Path<String>,
    client_data: web::Data<RealtimeClientConnections>,
) -> Result<HttpResponse, Error> {
    debug!("{:?}", r);
    let full_key = info.0;
    info!("Domain object requesting telemetry: {}", full_key);
    ws::start(RealtimeTelemetryProvider::new(full_key, client_data), &r, stream)
}

#[get("/injest/{full_key}")]
async fn injest_index(
    r: HttpRequest,
    stream: web::Payload,
    info: web::Path<String>,
    client_data: web::Data<RealtimeClientConnections>,
    db_data: web::Data<DBAddr>,
) -> Result<HttpResponse, Error> {
    let full_key = info.0;
    info!("New injest socket for: {}", full_key);
    ws::start(InjestSocket::new(full_key, client_data, db_data), &r, stream)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    
    #[cfg(debug_assertions)]
    std::env::set_var("RUST_LOG", "debug,actix_server=debug,actix_web=debug");
    
    #[cfg(not(debug_assertions))]
    std::env::set_var("RUST_LOG", "info,actix_server=info,actix_web=info");

    #[cfg(debug_assertions)]
    let root_dir = "ciudse-telemetry/dist/";
    
    #[cfg(not(debug_assertions))]
    let root_dir = "static/";
    
    env_logger::init();

    let realtime_connections = web::Data::new(RealtimeClientConnections::new());

    let db_data = web::Data::new(DBAddr::from(DBActor::new().start()));

    HttpServer::new(move || {
        App::new()
            .app_data(realtime_connections.clone())
            .app_data(db_data.clone())
            // enable logger
            .wrap(middleware::Logger::default())
            // websocket route
            .service(realtime_index)
            .service(injest_index)
            // static files
            .service(fs::Files::new("/", root_dir).index_file("index.html"))
    })
    .bind("127.0.0.1:8081")?
    .run()
    .await
}
