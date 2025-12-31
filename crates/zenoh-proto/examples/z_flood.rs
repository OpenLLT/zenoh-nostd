use std::{
    io::{Read, Write},
    net::TcpListener,
    time::Duration,
};

use zenoh_proto::{
    Transport,
    exts::QoS,
    fields::{Reliability, WireExpr},
    keyexpr,
    msgs::*,
};

fn handle_client(mut stream: std::net::TcpStream) {
    let mut transport = Transport::new([0; u16::MAX as usize]).listen().streamed();
    println!("Starting handshake...");
    // Handshake
    for _ in 0..2 {
        let mut scope = transport.scope();

        scope.rx.feed_stream(|data| {
            // In streamed mode we can just `read_exact`. Internally it will call this closure
            // twice, the first one to retrieve the length and the second one to retrieve the rest
            // of the data.
            println!("Awaiting {} bytes", data.len());
            stream.read_exact(data).expect("Couldn't raed");
            println!("Received {:?}", data)
        });

        for _ in scope.rx.flush(&mut scope.state) {}

        let bytes = scope
            .tx
            .interact(&mut scope.state)
            .expect("During listen handshake there should always be a response");

        println!("Sending bytes {:?}", bytes);
        stream.write_all(bytes).expect("Couldn't write");
    }

    assert!(transport.opened());
    println!("Handshake passed!");

    // Just send messages indefinitely
    let put = NetworkMessage {
        reliability: Reliability::default(),
        qos: QoS::default(),
        body: NetworkBody::Push(Push {
            wire_expr: WireExpr::from(keyexpr::from_str_unchecked("test/thr")),
            payload: PushBody::Put(Put {
                payload: &[0, 1, 2, 3, 4, 5, 6, 7],
                ..Default::default()
            }),
            ..Default::default()
        }),
    };

    for _ in 0..200 {
        transport.tx.push(&put).expect("Transport too small");
    }

    let bytes = transport.tx.flush().unwrap();

    loop {
        if stream.write_all(&bytes).is_err() {
            break;
        }
    }
}

fn main() {
    let listener = TcpListener::bind("127.0.0.1:7447").expect("Could not bind");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => handle_client(stream),
            Err(e) => {
                panic!("Error accepting connection: {}", e);
            }
        }
    }
}
