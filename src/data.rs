use actix::prelude::*;
use std::sync::Mutex;
use crate::actors::DBActor;

#[derive(Debug)]
pub struct DBAddr {
    pub addr: Mutex<Addr<DBActor>>
}

impl DBAddr {
    pub fn from(addr: Addr<DBActor>) -> DBAddr{
        DBAddr {
            addr: Mutex::new(addr),
        }
    }
}