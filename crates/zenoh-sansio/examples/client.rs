use std::{
    io::{Read, Write},
    net::TcpStream,
};
use zenoh_proto::Transport;

fn connect() -> std::io::Result<(TcpStream, Transport<[u8; 512]>)> {
    let mut stream = TcpStream::connect("127.0.0.1:7447")?;
    let mut transport = Transport::new([0u8; 512]).connect().streamed();
    if let Some(bytes) = transport.init() {
        stream.write_all(bytes)?;
    }

    for _ in 0..2 {
        let mut scope = transport.scope();

        if scope
            .rx
            .feed_with(|data| stream.read_exact(data).map_or(0, |_| data.len()))
            .is_err()
        {
            continue;
        }

        for _ in scope.rx.flush(&mut scope.state) {}

        if let Some(bytes) = scope.tx.interact(&mut scope.state) {
            stream.write_all(bytes).ok();
        }
    }

    assert!(transport.opened());

    Ok((stream, transport))
}

fn main() -> std::io::Result<()> {
    let (mut stream, mut transport) = connect()?;

    loop {
        if transport
            .rx
            .feed_with(|bytes| stream.read_exact(bytes).map_or(0, |_| bytes.len()))
            .is_err()
        {
            break;
        };

        let network_msgs = transport.rx.flush(&mut transport.state);
    }

    Ok(())
}
