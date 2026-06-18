use afl::fuzz;
use vox_types::{Message, MessagePayload, Payload, RequestBody};

fn can_serialize_after_decode(message: &Message<'_>) -> bool {
    match &message.payload {
        MessagePayload::RequestMessage(req) => match &req.body {
            RequestBody::Call(call) => matches!(call.args, Payload::Outgoing { .. }),
            RequestBody::Response(resp) => matches!(resp.ret, Payload::Outgoing { .. }),
            RequestBody::Cancel(_) => true,
        },
        MessagePayload::ChannelMessage(ch) => match &ch.body {
            vox_types::ChannelBody::Item(item) => matches!(item.item, Payload::Outgoing { .. }),
            vox_types::ChannelBody::Close(_) => true,
            vox_types::ChannelBody::Reset(_) => true,
            vox_types::ChannelBody::GrantCredit(_) => true,
        },
        _ => true,
    }
}

fn main() {
    fuzz!(|data: &[u8]| {
        let Ok(message) = vox::vox_postcard::from_slice_borrowed::<vox_types::Message<'_>>(data)
        else {
            return;
        };

        if can_serialize_after_decode(&message) {
            let _ = vox::vox_postcard::to_vec(&message);
        }
    });
}
