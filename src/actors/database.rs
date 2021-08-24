use std::error::Error;
use std::net::TcpStream;
use std::io::prelude::*;
use actix::prelude::*;
use crate::messages::PushDBMsg;
use log::{debug, warn};

#[derive(Debug)]
pub struct DBActor {
    stream: Option<TcpStream>,
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
            stream: None,
        }
    }

    fn pushdb(&mut self, msg: PushDBMsg) -> Result<String, Box<dyn Error>> {
        let database_address = "127.0.0.1:9009";
        if self.stream.is_none() {
            self.stream = Some(TcpStream::connect(database_address)?);
        }
        let mut stream = self.stream.as_ref().unwrap();
        // Influx line protocol timestamps are in nanoseconds
        let query = format!("{table} value={value} {timestamp}000000\n",
            table = msg.full_key,
            value = msg.value,
            timestamp = msg.timestamp, 
        );
        stream.write(query.as_bytes())?;
        Ok(query)
    }
}