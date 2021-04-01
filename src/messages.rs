use actix::prelude::*;

#[derive(Message, Debug, Clone)]
#[rtype("()")]
pub struct PushDBMsg {
    pub full_key: String,
    pub value: f32,
    pub timestamp: u64
}
