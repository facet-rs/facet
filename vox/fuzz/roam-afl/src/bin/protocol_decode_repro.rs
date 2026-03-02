use std::env;
use std::fs;

use roam_types::{Message, MessagePayload, Payload, RequestBody};

fn can_serialize_after_decode(message: &Message<'_>) -> bool {
    match &message.payload {
        MessagePayload::RequestMessage(req) => match &req.body {
            RequestBody::Call(call) => matches!(call.args, Payload::Outgoing { .. }),
            RequestBody::Response(resp) => matches!(resp.ret, Payload::Outgoing { .. }),
            RequestBody::Cancel(_) => true,
        },
        MessagePayload::ChannelMessage(ch) => match &ch.body {
            roam_types::ChannelBody::Item(item) => matches!(item.item, Payload::Outgoing { .. }),
            roam_types::ChannelBody::Close(_) => true,
            roam_types::ChannelBody::Reset(_) => true,
            roam_types::ChannelBody::GrantCredit(_) => true,
        },
        _ => true,
    }
}

fn main() {
    let mut args = env::args();
    let _exe = args.next();
    let path = args
        .next()
        .expect("usage: protocol_decode_repro <crash-file>");
    let data = fs::read(path).expect("read input file");

    let message = roam::facet_postcard::from_slice_borrowed::<roam_types::Message<'_>>(&data)
        .expect("decode should succeed for crashing input");
    if can_serialize_after_decode(&message) {
        let _encoded = roam::facet_postcard::to_vec(&message).expect("re-encode should succeed");
    }
}
