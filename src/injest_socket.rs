use crate::realtime_telemetry_provider::{RealtimeClientConnections, UpdateTelemetryMessage};
use actix::prelude::*;
use actix_web::web;
use actix_web_actors::ws;
use log::{debug, warn};
use serde_json::json;
use std::time::{Duration, Instant, UNIX_EPOCH};
use std::net::UdpSocket;
use std::error::Error;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub struct InjestSocket {
    last_heartbeat: Instant,
    full_key: String,
    data: web::Data<RealtimeClientConnections>,
}

impl Actor for InjestSocket {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.heartbeat(ctx);
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for InjestSocket {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        debug!("WS: {:?} [{}]", msg, self.full_key);
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                self.last_heartbeat = Instant::now();
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                self.last_heartbeat = Instant::now();
            }
            Ok(ws::Message::Text(text)) => {
                match self.handle_message(ctx, text){
                    Ok(_) => {},
                    Err(e) => { warn!("Error injesting data: {}", e); },
                };
            }
            Ok(ws::Message::Binary(_bin)) => {}
            Ok(ws::Message::Close(reason)) => {
                ctx.close(reason);
                ctx.stop();
            }
            _ => ctx.stop(),
        }
    }
}

impl InjestSocket {
    pub fn new(full_key: String, data: web::Data<RealtimeClientConnections>) -> Self {
        InjestSocket {
            last_heartbeat: Instant::now(),
            full_key: full_key,
            data: data,
        }
    }

    fn heartbeat(&self, ctx: &mut <Self as Actor>::Context) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            if Instant::now().duration_since(act.last_heartbeat) > CLIENT_TIMEOUT {
                // heartbeat timed out
                warn!("Websocket Client heartbeat failed, disconnecting!");
                // stop actor
                ctx.stop();
                // don't try to send a ping
                return;
            }
            ctx.ping(b"");
        });
    }

    fn handle_message(&mut self, _ctx: &mut <Self as Actor>::Context, msg: String) -> Result<(), Box<dyn Error>> {
        let value = msg.parse::<f32>()?;

        // ? Will we ever get that far ?
        let timestamp = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;

        if let Ok(client_sockets) = self.data.sockets.lock() {
            
            let message = UpdateTelemetryMessage::from(json!({
                "timestamp": timestamp,
                "value": value
            }));

            client_sockets.get(&self.full_key).map(|key_client_sockets| {
                for addr in key_client_sockets {
                    debug!("Sending message [{}] to: {:?}", self.full_key, addr);
                    addr.do_send(message.clone());
                }
            });

        } else {
            return Err("Couldn't acquire lock!".into());
        }
        
        Ok(())
    }
}
