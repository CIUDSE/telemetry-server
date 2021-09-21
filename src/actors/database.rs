use std::{collections::btree_set::Union, error::Error};
use std::net::TcpStream;
use std::io::prelude::*;
use actix::prelude::*;
use crate::messages::*;
use log::{debug, warn};
use crate::data::TelemetryDatum;
use serde_json::Value;
use ms_converter;

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

impl Handler<QueryDBMsg> for DBActor {
    type Result = ResponseFuture<Result<Vec<TelemetryDatum>, ()>>;

    fn handle(
        &mut self,
        msg: QueryDBMsg,
        _ctx: &mut <Self as Actor>::Context) -> Self::Result
    {
        Box::pin(async move {
            let re = querydb_suppress_errors(msg);
            let re = re.await;
            Ok(re)
        })
    }
}

impl Actor for DBActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Context<Self>) {
        debug!("Database actor started!");
    }
}

async fn querydb_suppress_errors(msg: QueryDBMsg) -> Vec<TelemetryDatum> {
    match querydb(msg).await {
        Ok(r) => r,
        Err(e) => vec![],
    }
}

async fn querydb(msg: QueryDBMsg) -> Result<Vec<TelemetryDatum>, Box<dyn Error>> {
    let database_url = "127.0.0.1:9000/exec";
    let full_key = msg.full_key;
    let start = msg.start;
    let end = msg.end;
    let sql_query = format!(
        "SELECT * FROM \"{table}\" WHERE timestamp BETWEEN CAST({left_millis}000000 AS TIMESTAMP) AND CAST({right_millis}000000 AS TIMESTAMP)",
        table = full_key,
        left_millis = start,
        right_millis = end
    );
    let mut response = actix_web::client::Client::new().get(database_url).query(&sql_query)?.send().await?;
    let raw_data = response.body().await?;
    let data: Value = serde_json::from_slice(&raw_data)?;
    let dataset = &data["dataset"];
    let mut output: Vec<TelemetryDatum> = Vec::new();
    for row in dataset.as_array().unwrap() {
        let val = row.as_array().unwrap()[0].as_f64().unwrap();
        let timestamp = row.as_array().unwrap()[1].as_str().unwrap();
        let timestamp = ms_converter::ms(timestamp).unwrap() as u64;
        output.push(TelemetryDatum{
            timestamp,
            value: val,
        });
    }
    Ok(output)
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
        let query = format!("{table} value={value} {timestamp}000000\n\n",
            table = msg.full_key,
            value = msg.value,
            timestamp = msg.timestamp, 
        );
        stream.write(query.as_bytes())?;
        Ok(query)
    }
}