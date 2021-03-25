use actix_files as fs;
use actix_web::{get, middleware, web, App, Error, HttpRequest, HttpResponse, HttpServer};

mod realtime_telemetry_provider;
use actix_web_actors::ws;
use log::{debug, info};
use realtime_telemetry_provider::{RealtimeClientConnections, RealtimeTelemetryProvider};
mod injest_socket;
use injest_socket::InjestSocket;

#[get("/realtime/{full_key}")]
async fn realtime_index(
    r: HttpRequest,
    stream: web::Payload,
    info: web::Path<String>,
    data: web::Data<RealtimeClientConnections>,
) -> Result<HttpResponse, Error> {
    debug!("{:?}", r);
    let full_key = info.0;
    info!("Domain object requesting telemetry: {}", full_key);
    ws::start(RealtimeTelemetryProvider::new(full_key, data), &r, stream)
}

#[get("/injest/{full_key}")]
async fn injest_index(
    r: HttpRequest,
    stream: web::Payload,
    info: web::Path<String>,
    data: web::Data<RealtimeClientConnections>,
) -> Result<HttpResponse, Error> {
    let full_key = info.0;
    info!("New injest socket for: {}", full_key);
    ws::start(InjestSocket::new(full_key, data), &r, stream)
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

    HttpServer::new(move || {
        App::new()
            .app_data(realtime_connections.clone())
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
