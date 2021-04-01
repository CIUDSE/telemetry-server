use std::error::Error;
use std::net::UdpSocket;
use actix::prelude::*;
use crate::messages::PushDBMsg;
use log::{debug, warn};

#[derive(Debug)]
pub struct DBActor {
    socket: Option<UdpSocket>,
}

impl Handler<PushDBMsg> for DBActor {
    type Result = ();

    fn handle(
        &mut self,
        msg: PushDBMsg,
        _ctx: &mut <Self as Actor>::Context)
    {
        match self.pushdb(msg) {
            Ok(r) => { debug!("{}", r); },
            Err(e) => { warn!("{:?}", e); }
        };
    }
}

impl Actor for DBActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Context<Self>) {
        debug!("Database actor started!");
    }
}

impl DBActor {
    pub fn new() -> DBActor {
        DBActor {
            socket: None,
        }
    }

    fn pushdb(&self, msg: PushDBMsg) -> Result<usize, Box<dyn Error>> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        let query = format!("{table} value={value} {timestamp}",
            table = msg.full_key,
            value = msg.value,
            timestamp = msg.timestamp, 
        );
        let database_address = "127.0.0.1:9009";
        socket.connect(database_address)?;
        let r = socket.send(query.as_bytes())?;
        Ok(r)
    }
}