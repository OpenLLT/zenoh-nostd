use std::{
    io::{Read, Write},
    net::TcpStream,
};

enum Packet {
    Init,
    Put(&'static str, &'static [u8]),
    Sub(&'static str),
}

impl Packet {
    fn payload(&self) -> &[u8] {
        &[]
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

fn client1() {
    let mut conn = Connection::new();
    let mut stream = TcpStream::connect("127.0.0.1:7447").unwrap();

    let init_pckt = conn.init();
    write_tcp!(stream, init_pckt.payload());

    let mut rx = [0u8; 1024];
    loop {
        let events = conn.recv(read_tcp!(stream, rx));

        for event in events {
            match event {
                Event::SendPacket(packet) => {
                    write_tcp!(stream, packet.payload());
                }
                Event::SubscriberReceived {
                    id,
                    keyexpr,
                    payload,
                } => {
                    println!(
                        "Client1 received on subscriber {}: {} -> {:?}",
                        id, keyexpr, payload
                    );
                }
            }
        }

        if !conn.established() {
            continue;
        }

        let sub_packet = Packet::Sub("demo/example/data");
        write_tcp!(stream, sub_packet.payload());
    }
}

fn client2() {
    let mut conn = Connection::new();
    let listener = std::net::TcpListener::bind("127.0.0.1:7447").unwrap();
    let mut stream = listener.accept().unwrap().0;

    let mut rx = [0u8; 1024];
    loop {
        let events = conn.recv(read_tcp!(stream, rx));

        for event in events {
            match event {
                Event::SendPacket(packet) => {
                    write_tcp!(stream, packet.payload());
                }
                Event::SubscriberReceived {
                    id,
                    keyexpr,
                    payload,
                } => {
                    println!(
                        "Client2 received on subscriber {}: {} -> {:?}",
                        id, keyexpr, payload
                    );
                }
            }
        }

        if !conn.established() {
            continue;
        }

        let put_packet = Packet::Put("demo/example/data", b"Hello from client2");
        write_tcp!(stream, put_packet.payload());
    }
}

fn main() {
    std::thread::spawn(|| {
        client2();
    });

    std::thread::sleep(std::time::Duration::from_millis(100));

    client1();

    let mut t = Test::<u8>::new(5u8);
    t.recv(9u8);

    let mut t2 = Test::<u16>::new(500u16);
    t2.recv(900u16, 1usize);
}

struct Test<T> {
    value: T,
}

impl Test<u8> {
    fn new(value: u8) -> Self {
        Self { value }
    }

    fn recv(&mut self, data: u8) {
        self.value = data;
    }
}

impl Test<u16> {
    fn new(value: u16) -> Self {
        Self { value }
    }

    fn recv(&mut self, data: u16, id: usize) {
        self.value = data;
    }
}
