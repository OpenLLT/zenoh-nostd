use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
};

struct Broker {}

impl Broker {
    fn new() -> Self {
        Self {}
    }

    fn init(&self) -> Packet {
        Packet::Init
    }

    fn recv(&mut self, _data: &[u8], _source_id: usize) -> impl Iterator<Item = Event> {
        core::iter::empty()
    }
}

enum Packet {
    Init,
    Put,
    Sub,
}

impl Packet {
    fn payload(&self) -> &[u8] {
        &[]
    }

    fn id(&self) -> u32 {
        0
    }
}

enum Event {
    SendPacket(Packet),
    SubscriberReceived {
        id: u32,
        keyexpr: String,
        payload: Vec<u8>,
    },
}

impl Event {
    fn payload(&self) -> &[u8] {
        &[]
    }
}

struct Connection {}

impl Connection {
    fn new() -> Self {
        Self {}
    }

    fn id(&self) -> u32 {
        0
    }

    fn init(&self) -> Packet {
        Packet::Init
    }

    fn recv(&mut self, _data: &[u8]) -> impl Iterator<Item = Event> {
        core::iter::empty()
    }

    fn established(&self) -> bool {
        true
    }

    fn put(&mut self, _keyexpr: &str, _payload: &[u8]) -> Packet {
        Packet::Put
    }

    fn subscribe(&mut self, _keyexpr: &str) -> Packet {
        Packet::Sub
    }
}

macro_rules! read_tcp {
    ($stream:expr, $buf:expr) => {{
        let mut l = [0u8; 2];
        $stream.read_exact(&mut l).unwrap();
        let l = u16::from_le_bytes(l) as usize;
        $stream.read_exact(&mut $buf[..l]).unwrap();
        &$buf[..l]
    }};
}

macro_rules! write_tcp {
    ($stream:expr, $buf:expr) => {{
        let l = $buf.len() as u16;
        $stream.write_all(&l.to_le_bytes()).unwrap();
        $stream.write_all($buf).unwrap();
    }};
}

// A macro that tries to read on 3 TCP streams
macro_rules! try_read_tcp3 {
    ($stream1:expr, $stream2:expr, $stream3:expr, $buf:expr) => {{
        if let Ok(_) = $stream1.read_exact(&mut [0u8; 0]) {
            let data = read_tcp!($stream1, $buf);
            Some((1, data))
        } else if let Ok(_) = $stream2.read_exact(&mut [0u8; 0]) {
            let data = read_tcp!($stream2, $buf);
            Some((2, data))
        } else if let Ok(_) = $stream3.read_exact(&mut [0u8; 0]) {
            let data = read_tcp!($stream3, $buf);
            Some((3, data))
        } else {
            None
        }
    }};
}

fn broker() {
    let mut broker = Broker::new();

    let mut gateway_stream = TcpStream::connect("127.0.0.1:7447").unwrap();
    let init_pckt = broker.init();
    write_tcp!(gateway_stream, init_pckt.payload());

    let left = TcpListener::bind("127.0.0.1:7555").unwrap();
    let right = TcpListener::bind("127.0.0.1:7556").unwrap();

    let mut left_stream = left.accept().unwrap().0;
    let mut right_stream = right.accept().unwrap().0;

    gateway_stream.set_nonblocking(true).unwrap();
    left_stream.set_nonblocking(true).unwrap();
    right_stream.set_nonblocking(true).unwrap();

    let mut rx = [0u8; 1024];
    loop {
        let (id, data) =
            try_read_tcp3!(&mut gateway_stream, &mut left_stream, &mut right_stream, rx).unwrap();

        let events = broker.recv(data, id);

        for event in events {
            match event {
                Event::SendPacket(packet) => match packet.id() {
                    0 => write_tcp!(gateway_stream, packet.payload()),
                    1 => write_tcp!(left_stream, packet.payload()),
                    2 => write_tcp!(right_stream, packet.payload()),
                    _ => {}
                },
                _ => {}
            }
        }
    }
}

fn main() {}
