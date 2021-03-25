use std::time::{Duration, Instant, UNIX_EPOCH};
use log::{debug, warn};
use actix::prelude::*;
use actix_web::web;
use actix_web_actors::ws;
use serde_json::json;
use crate::realtime_telemetry_provider::{RealtimeClientConnections, UpdateTelemetryMessage};

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
    fn handle(
        &mut self,
        msg: Result<ws::Message, ws::ProtocolError>,
        ctx: &mut Self::Context,
    ) {
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
                self.handle_message(ctx, text);
            },
            Ok(ws::Message::Binary(_bin)) => {},
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
        InjestSocket{
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

    fn handle_message(&mut self, _ctx: &mut <Self as Actor>::Context, msg: String) {
        let value = msg.parse::<f32>();
        if value.is_err() {
            warn!("Socket message is not valid value: {}", msg);
            return
        }
        let value = value.unwrap();
        let sockets = self.data.sockets.lock();
        if sockets.is_err() {
            warn!("Couldn't aquire socket list lock");
            return
        }
        let sockets = sockets.unwrap();
        let key_sockets = sockets.get(&self.full_key);
        if key_sockets.is_none() {
            debug!("No registered client sockets for point [{}]", self.full_key);
            return
        }
        let key_sockets = key_sockets.unwrap();


        let timestamp = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;
        let message = UpdateTelemetryMessage::from(json!({
            "timestamp": timestamp,
            "value": value
        }));
        debug!("Message: {:?}", message);
        for addr in key_sockets {
            debug!("Sending message [{}] to: {:?}", self.full_key, addr);
            addr.do_send(message.clone());
        }
    }
}
