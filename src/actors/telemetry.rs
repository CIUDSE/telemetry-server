use std::{
    error::Error,
    time::{Duration, Instant, UNIX_EPOCH},
    collections::HashSet
};
use actix::prelude::*;
use actix_web::web;
use actix_web_actors::ws;
use log::{debug, warn};
use serde_json::json;
use crate::messages::*;
use crate::data::*;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub struct InjestSocket {
    last_heartbeat: Instant,
    full_key: String,
    client_data: web::Data<RealtimeClientConnections>,
    db_data: web::Data<DBAddr>
}

impl Actor for InjestSocket {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.heartbeat(ctx);
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for InjestSocket {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        //debug!("WS: {:?} [{}]", msg, self.full_key);
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                self.last_heartbeat = Instant::now();
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                self.last_heartbeat = Instant::now();
            }
            Ok(ws::Message::Text(text)) => {
                match self.injest_data(ctx, text){
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
    pub fn new(full_key: String, client_data: web::Data<RealtimeClientConnections>, db_data: web::Data<DBAddr>) -> Self {
        InjestSocket {
            last_heartbeat: Instant::now(),
            full_key,
            client_data,
            db_data,
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

    fn injest_data(&mut self, _ctx: &mut <Self as Actor>::Context, msg: String) -> Result<(), Box<dyn Error>> {
        let value = msg.parse::<f32>()?;

        // ? Will we ever get that far ?
        let timestamp = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;

        self.db_data.addr.lock().unwrap().do_send(PushDBMsg {
            full_key: self.full_key.clone(),
            value: value,
            timestamp: timestamp
        });

        // For some reason using "?" doesn't work here
        if let Ok(client_sockets) = self.client_data.sockets.lock() {
            if let Some(key_client_sockets) = client_sockets.get(&self.full_key) {
                let message = UpdateTelemetryMessage::from(json!({
                    "timestamp": timestamp,
                    "value": value
                }));
                for addr in key_client_sockets {
                    debug!("Sending message [{}] to: {:?}", self.full_key, addr);
                    addr.do_send(message.clone());
                }
            }
        } else {
            return Err("Couldn't acquire lock!".into());
        }
        
        Ok(())
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
        let mut sockets = match self.data.sockets.lock() {
            Ok(x) => x,
            Err(x) => {
                warn!("Couldn't aquire lock to add client socket to list! Closing connection client...");
                warn!("{:?}", x);
                ctx.close(Some(ws::CloseReason {
                    code: ws::CloseCode::Error,
                    description: Some("Internal server error. Couldn't register client.".to_string()),
                }));
                return;
            }
        };
        sockets
            .entry(self.full_key.clone())
            .or_insert(HashSet::new())
            .insert(addr);
        debug!("Adding new socket to list. Current Sockets [{}]: {:?}", sockets.len(), sockets);
    }

    fn stopping(&mut self, ctx: &mut Self::Context) -> Running {
        // Unregister client from global state
        let addr = ctx.address();
        let mut sockets = match self.data.sockets.lock(){
            Ok(x) => x,
            Err(x) => {
                warn!("Couldn't aquire lock to remove client from socket list! This may cause an error if sending data to stopped actor address");
                warn!("{:?}", x);
                // ? What should we do here? Is Actix web smart enough to ignore messages to stopped actor?
                // ? Maybe resume and try to aquire lock later?
                // For now just stop
                return Running::Stop;
                // TODO: Figure out solution
            }
        };
        sockets
            .entry(self.full_key.clone())
            .or_insert(HashSet::new())
            .remove(&addr);
        debug!("Removing socket from list. Current Sockets [{}]: {:?}", sockets.len(), sockets);
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
            full_key,
            data,
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
