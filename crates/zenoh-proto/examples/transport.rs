use std::io::Write;

use zenoh_proto::{
    Transport,
    exts::QoS,
    fields::{Reliability, WireExpr},
    keyexpr,
    msgs::{MessageIter, NetworkBody, NetworkMessage, Push},
};

fn main() {
    let mut transport = Transport::new(false, [0u8; 512], [0u8; 512], 512 as u16, 0);

    let push = NetworkMessage {
        qos: QoS::default(),
        reliability: Reliability::default(),
        body: NetworkBody::Push(Push {
            wire_expr: WireExpr::from(keyexpr::from_str_unchecked("ab/cdef")),
            ..Default::default()
        }),
    };

    println!("Created NetworkMessage: {:?}", push);

    let mut data = [0u8; 512];
    let mut w = &mut data[..];
    let slice = transport
        .tx()
        .send(core::iter::empty(), core::iter::once(push))
        .next()
        .expect("Should have at least one chunk");

    w[..slice.len()].copy_from_slice(slice);
    w = &mut w[..slice.len()];

    println!("Encoded data {:?}", &w[..]);

    let (mut rx, _) = transport.update(|d| {
        d[..w.len()].copy_from_slice(&w[..]);
        w.len()
    });

    let mut batch = match rx.next().expect("Should have at least one batch") {
        MessageIter::Network(msgs) => msgs,
        _ => panic!("Expected NetworkMessage"),
    };

    let msg = batch.next().expect("Should have at least one message");
    println!("Decoded NetworkMessage: {:?}", msg);
}
