// CITA
// Copyright 2016-2017 Cryptape Technologies LLC.

// This program is free software: you can redistribute it
// and/or modify it under the terms of the GNU General Public
// License as published by the Free Software Foundation,
// either version 3 of the License, or (at your option) any
// later version.

// This program is distributed in the hope that it will be
// useful, but WITHOUT ANY WARRANTY; without even the implied
// warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR
// PURPOSE. See the GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

use amqp::{Basic, Channel, Consumer, protocol};
use citaprotocol::CitaRequest;
use connection::Connection;
use libproto::*;
use libproto::communication::*;
use libproto::request::Request;
use protobuf::Message;
use protobuf::core::parse_from_bytes;
use pubsub::Pub;
use server::MySender;
use std::io;
use std::sync::Arc;
use std::sync::mpsc::Sender;

pub struct MyHandler {
    con: Arc<Connection>,
    puber: Pub,
    ctx: Sender<communication::Message>,
}

impl MyHandler {
    pub fn new(con: Arc<Connection>, puber: Pub, ctx: Sender<communication::Message>) -> Self {
        MyHandler { con: con, puber: puber, ctx: ctx }
    }
}

impl Consumer for MyHandler {
    fn handle_delivery(&mut self, channel: &mut Channel, deliver: protocol::basic::Deliver, _: protocol::basic::BasicProperties, body: Vec<u8>) {
        trace!("handle delivery id {:?} payload {:?}", deliver.routing_key, body);
        if let (_, true, msg) = is_need_proc(body.as_ref()) {
            self.ctx.send(msg).unwrap();
        }
        handle_rpc(&self.con, &mut self.puber, body.as_ref());
        let _ = channel.basic_ack(deliver.delivery_tag, false);
    }
}

fn handle_rpc(con: &Connection, puber: &mut Pub, payload: &[u8]) {
    if let Ok(msg) = parse_from_bytes::<communication::Message>(payload) {
        let t = msg.get_field_type();
        let cid = msg.get_cmd_id();
        trace!("recive MQ messsage from {:?} module", display_cmd(cid));
        if cid == cmd_id(submodules::JSON_RPC, topics::REQUEST) && t == MsgType::REQUEST {
            let mut ts = parse_from_bytes::<Request>(msg.get_content()).unwrap();
            let mut response = request::Response::new();
            response.set_request_id(ts.take_request_id());
            if ts.has_peercount() {
                let peercount = con.peers_pair.iter().filter(|x| x.2.as_ref().read().is_some()).count();
                response.set_peercount(peercount as u32);
                let ms: communication::Message = response.into();
                puber.publish("chain.rpc", ms.write_to_bytes().unwrap());
            }
        }
    }
}

fn is_need_proc(payload: &[u8]) -> (String, bool, communication::Message) {
    if let Ok(msg) = parse_from_bytes::<communication::Message>(payload) {
        let mut topic = String::default();
        let mut is_proc = true;
        let t = msg.get_field_type();
        let cid = msg.get_cmd_id();
        if cid == cmd_id(submodules::CONSENSUS, topics::NEW_TX) && t == MsgType::TX {
            trace!("CONSENSUS broadcast tx");
            topic = "net.tx".to_string();
        } else if cid == cmd_id(submodules::CONSENSUS, topics::NEW_BLK) && t == MsgType::BLOCK {
            info!("CONSENSUS pub blk");
            topic = "net.blk".to_string();
        } else if cid == cmd_id(submodules::CHAIN, topics::NEW_BLK) && t == MsgType::BLOCK {
            info!("CHAIN pub blk");
            topic = "net.blk".to_string();
        } else if cid == cmd_id(submodules::CHAIN, topics::NEW_STATUS) && t == MsgType::STATUS {
            info!("CHAIN pub status");
            topic = "net.status".to_string();
        } else if cid == cmd_id(submodules::CHAIN, topics::SYNC_BLK) && t == MsgType::MSG {
            info!("CHAIN sync blk");
            topic = "net.sync".to_string();
        } else if (cid == cmd_id(submodules::CONSENSUS, topics::CONSENSUS_MSG) && t == MsgType::MSG) || (cid == cmd_id(submodules::CONSENSUS, topics::NEW_PROPOSAL) && t == MsgType::MSG) {
            trace!("CONSENSUS pub msg");
            topic = "net.msg".to_string();
        } else {
            is_proc = false;
        }
        return (topic, is_proc, msg);
    }
    ("".to_string(), false, communication::Message::new())
}

pub fn net_msg_handler(payload: CitaRequest, mysender: &MySender) -> Result<Vec<u8>, io::Error> {
    trace!("SERVER get msg: {:?}", payload);
    if let (topic, true, msg) = is_need_proc(payload.as_ref()) {
        info!("recive msg from origin = {:?}", msg.get_origin());
        mysender.send((topic, payload))
    }
    Ok(vec![])
}
