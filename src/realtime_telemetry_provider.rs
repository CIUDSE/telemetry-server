use actix::prelude::*;
use actix_web::web;
use actix_web_actors::ws;
use log::{debug, warn};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub struct RealtimeClientConnections {
    pub sockets: Mutex<HashMap<String, HashSet<Addr<RealtimeTelemetryProvider>>>>,
}

impl RealtimeClientConnections {
    pub fn new() -> Self {
        Self {
            sockets: Mutex::new(HashMap::new()),
        }
    }
}

#[derive(Message, Debug, Clone)]
#[rtype("()")]
pub struct UpdateTelemetryMessage {
    json_data: serde_json::Value,
}

impl UpdateTelemetryMessage {
    pub fn from(data: serde_json::Value) -> UpdateTelemetryMessage {
        UpdateTelemetryMessage { json_data: data }
    }
}

#[derive(Debug)]
pub struct RealtimeTelemetryProvider {
    last_heartbeat: Instant,
    full_key: String,
    data: web::Data<RealtimeClientConnections>,
}

impl Actor for RealtimeTelemetryProvider {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        // Heartbeat
        self.heartbeat(ctx);
        // Register client in global state
        let addr = ctx.address();
        let sockets = self.data.sockets.lock();
        if sockets.is_err() {
            warn!(
                "Couldn't aquire lock to add client socket to list! Closing connection client..."
            );
            ctx.close(Some(ws::CloseReason {
                code: ws::CloseCode::Error,
                description: Some("Internal server error. Couldn't register client.".to_string()),
            }));
            return;
        }
        let mut sockets = sockets.unwrap();
        sockets
            .entry(self.full_key.clone())
            .or_insert(HashSet::new())
            .insert(addr);
        debug!(
            "Adding new socket to list. Current Sockets [{}]: {:?}",
            sockets.len(),
            sockets
        );
    }

    fn stopping(&mut self, ctx: &mut Self::Context) -> Running {
        // Unregister client from global state
        let addr = ctx.address();
        let sockets = self.data.sockets.lock();
        if sockets.is_err() {
            warn!("Couldn't aquire lock to remove client from socket list! This may cause an error if sending data to stopped actor address");
            // ? What should we do here? Is Actix web smart enough to ignore messages to stopped actor?
            // ? Maybe resume and try to aquire lock later?
            // For now just stop
            return Running::Stop;
            // TODO: Figure out solution
        }
        let mut sockets = sockets.unwrap();
        sockets
            .entry(self.full_key.clone())
            .or_insert(HashSet::new())
            .remove(&addr);
        debug!(
            "Removing socket from list. Current Sockets [{}]: {:?}",
            sockets.len(),
            sockets
        );
        Running::Stop
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for RealtimeTelemetryProvider {
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
            Ok(ws::Message::Text(_text)) => {}
            Ok(ws::Message::Binary(_bin)) => {}
            Ok(ws::Message::Close(reason)) => {
                ctx.close(reason);
                ctx.stop();
            }
            _ => ctx.stop(),
        }
    }
}

impl Handler<UpdateTelemetryMessage> for RealtimeTelemetryProvider {
    type Result = ();

    fn handle(
        &mut self,
        msg: UpdateTelemetryMessage,
        ctx: &mut <Self as Actor>::Context,
    ) -> Self::Result {
        ctx.text(msg.json_data.to_string());
    }
}

impl RealtimeTelemetryProvider {
    pub fn new(full_key: String, data: web::Data<RealtimeClientConnections>) -> Self {
        RealtimeTelemetryProvider {
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
}
