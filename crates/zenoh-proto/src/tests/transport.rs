use core::time::Duration;
use rand::{Rng, thread_rng};

use crate::{establishment::State, exts::*, fields::*, msgs::*, *};

const NUM_ITER: usize = 100;
const MAX_PAYLOAD_SIZE: usize = 512;

#[test]
fn transport_state_handshake_regular() {
    let mut a = State::WaitingInitSyn {
        mine_zid: ZenohIdProto::default(),
        mine_batch_size: 512,
        mine_resolution: Resolution::default(),
        mine_lease: Duration::from_secs(30),
    };

    let b_zid = ZenohIdProto::default();
    let mut b = State::WaitingInitAck {
        mine_zid: b_zid,
        mine_batch_size: 1025,
        mine_resolution: Resolution::default(),
        mine_lease: Duration::from_secs(37),
    };

    let mut buff1 = [0u8; 512];
    let mut buff2 = [0u8; 512];

    // Simulate 'b' sending InitSyn
    let mut len: usize = crate::codec::transport_encoder(
        &mut buff1,
        core::iter::once(TransportMessage::InitSyn(InitSyn {
            identifier: InitIdentifier {
                zid: b_zid,
                ..Default::default()
            },
            resolution: InitResolution {
                resolution: Resolution::default(),
                batch_size: BatchSize(1025),
            },
            ..Default::default()
        })),
    )
    .sum();

    // a receives InitSyn and goes to WaitingOpenSyn state and writes an InitAck on buff2
    a.poll(&buff1[..len], (&mut buff2, &mut len));
    // b receives an InitAck and goes to WaitingOpenAck and writes an OpenSyn on buff1
    b.poll(&buff2[..len], (&mut buff1, &mut len));
    // a receives an OpenSyn and goes to Opened and writes an OpenAck on buff2
    a.poll(&buff1[..len], (&mut buff2, &mut len));
    // b receives an OpenAck and goes to Opened
    b.poll(&buff2[..len], (&mut buff1, &mut len));

    assert!(a.opened() && b.opened());
}

#[test]
fn transport_state_handshake_0rtt() {
    let mut a = State::WaitingInitSyn {
        mine_zid: ZenohIdProto::default(),
        mine_batch_size: 512,
        mine_resolution: Resolution::default(),
        mine_lease: Duration::from_secs(30),
    };

    let mut socket = [0u8; 512];
    let writer = &mut socket[..];

    let mut len = crate::codec::transport_encoder(
        writer,
        core::iter::once(TransportMessage::InitSyn(InitSyn::default())),
    )
    .sum::<usize>();

    let (cookie, remain) = writer.split_at_mut(len);
    len += crate::codec::transport_encoder(
        remain,
        core::iter::once(TransportMessage::OpenSyn(OpenSyn {
            cookie,
            ..Default::default()
        })),
    )
    .sum::<usize>();

    a.poll(&socket[..len], (&mut [0u8; 512], &mut 0));
    assert!(a.opened())
}

#[test]
fn transport_non_streamed() {
    let mut tx = TransportTx::new(
        [0u8; 512],
        false,
        512,
        0,
        Resolution::default(),
        Duration::from_secs(10),
    );

    let mut rx = TransportRx::new(
        [0u8; 512],
        false,
        512,
        0,
        Resolution::default(),
        Duration::from_secs(10),
    );

    let msg = NetworkMessage {
        reliability: Reliability::Reliable,
        qos: QoS::default(),
        body: NetworkBody::RawBody(RawBody {
            buff: &[1, 2, 9, 8, 7],
        }),
    };

    tx.encode_ref(core::iter::once(&msg)).unwrap();

    extern crate std;
    rx.decode_with(|read: &mut [u8]| {
        if let Some(bytes) = tx.flush() {
            read[..bytes.len()].copy_from_slice(bytes);
            Ok::<usize, usize>(bytes.len())
        } else {
            Ok::<usize, usize>(0)
        }
    })
    .unwrap();

    let mut flush = rx.flush();
    let m = flush.next().unwrap();
    assert_eq!(flush.count(), 0);

    assert_eq!(m, msg);
}

#[test]
fn transport_streamed_decode() {
    let mut tx = TransportTx::new(
        [0u8; 512],
        true,
        512,
        0,
        Resolution::default(),
        Duration::from_secs(10),
    );

    let mut rx = TransportRx::new(
        [0u8; 512],
        true,
        512,
        0,
        Resolution::default(),
        Duration::from_secs(10),
    );

    let msg = NetworkMessage {
        reliability: Reliability::Reliable,
        qos: QoS::declare(),
        body: NetworkBody::RawBody(RawBody {
            buff: &[1, 2, 9, 8, 7],
        }),
    };

    tx.encode_ref(core::iter::once(&msg)).unwrap();
    rx.decode(tx.flush().unwrap()).unwrap();

    let mut flush = rx.flush();
    let m = flush.next().unwrap();
    assert_eq!(flush.count(), 0);

    assert_eq!(m, msg);
}

#[test]
fn transport_streamed_decode_with() {
    let mut tx = TransportTx::new(
        [0u8; 512],
        true,
        512,
        0,
        Resolution::default(),
        Duration::from_secs(10),
    );

    let mut rx = TransportRx::new(
        [0u8; 512],
        true,
        512,
        0,
        Resolution::default(),
        Duration::from_secs(10),
    );

    let msg = NetworkMessage {
        reliability: Reliability::Reliable,
        qos: QoS::declare(),
        body: NetworkBody::RawBody(RawBody {
            buff: &[1, 2, 9, 8, 7],
        }),
    };

    tx.encode_ref(core::iter::once(&msg)).unwrap();

    let mut bytes = tx.flush().unwrap();
    // In case of streamed value, this function will be called twice
    rx.decode_with(|read: &mut [u8]| {
        let (ret, remain) = bytes.split_at(read.len());
        read.copy_from_slice(ret);
        bytes = remain;
        Ok::<usize, usize>(ret.len())
    })
    .unwrap();

    let mut flush = rx.flush();
    let m = flush.next().unwrap();
    assert_eq!(flush.count(), 0);

    assert_eq!(m, msg);
}

fn net_rand<'a>(w: &mut impl crate::ZStoreable<'a>) -> NetworkMessage<'a> {
    use rand::seq::SliceRandom;
    let mut rng = rand::thread_rng();
    let choices = [
        Push::ID,
        Request::ID,
        Response::ID,
        ResponseFinal::ID,
        Interest::ID,
        Declare::ID,
    ];

    let body = match *choices.choose(&mut rng).unwrap() {
        Push::ID => NetworkBody::Push(Push::rand(w)),
        Request::ID => NetworkBody::Request(Request::rand(w)),
        Response::ID => NetworkBody::Response(Response::rand(w)),
        ResponseFinal::ID => NetworkBody::ResponseFinal(ResponseFinal::rand(w)),
        Interest::ID => {
            if rng.gen_bool(0.5) {
                NetworkBody::Interest(Interest::rand(w))
            } else {
                NetworkBody::InterestFinal(InterestFinal::rand(w))
            }
        }
        Declare::ID => NetworkBody::Declare(Declare::rand(w)),
        _ => unreachable!(),
    };

    NetworkMessage {
        reliability: Reliability::rand(w),
        qos: QoS::rand(w),
        body,
    }
}

#[test]
fn transport_codec_non_streamed() {
    extern crate std;
    use std::collections::VecDeque;

    let mut rand = [0u8; MAX_PAYLOAD_SIZE * NUM_ITER];
    let mut rw = rand.as_mut_slice();

    let mut messages = {
        let mut msgs = VecDeque::new();
        for _ in 0..thread_rng().gen_range(1..16) {
            msgs.push_back(net_rand(&mut rw));
        }
        msgs
    };

    let mut transport = Transport::new([0u8; MAX_PAYLOAD_SIZE * NUM_ITER]).codec();

    transport.tx.encode_ref(messages.iter()).unwrap();
    transport.rx.decode(transport.tx.flush().unwrap()).unwrap();

    for msg in transport.rx.flush() {
        let actual = messages.pop_front().unwrap();
        assert_eq!(msg, actual);
    }

    assert!(messages.is_empty());
}

#[test]
fn transport_codec_streamed() {
    extern crate std;
    use std::collections::VecDeque;

    let mut rand = [0u8; MAX_PAYLOAD_SIZE * NUM_ITER];
    let mut rw = rand.as_mut_slice();

    let mut messages = {
        let mut msgs = VecDeque::new();
        for _ in 0..thread_rng().gen_range(1..16) {
            msgs.push_back(net_rand(&mut rw));
        }
        msgs
    };

    let mut transport = Transport::new([0u8; MAX_PAYLOAD_SIZE * NUM_ITER])
        .codec()
        .streamed();

    transport.tx.encode_ref(messages.iter()).unwrap();
    transport.rx.decode(transport.tx.flush().unwrap()).unwrap();

    for msg in transport.rx.flush() {
        let actual = messages.pop_front().unwrap();
        assert_eq!(msg, actual);
    }

    assert!(messages.is_empty());
}

fn transport_handshake() {
    let mut t1 = Transport::new([0u8; MAX_PAYLOAD_SIZE * NUM_ITER]).connect(
        |read: &mut [u8]| {
            let _ = 3;
            Ok::<usize, usize>(0)
        },
        |write: &[u8]| {
            let _ = 3;
            Ok::<(), usize>(())
        },
    );
    // let mut t2 = Transport::new([0u8; MAX_PAYLOAD_SIZE * NUM_ITER]).listen();
}

// fn dumb_handshake<const N: usize>(t1: &mut Transport<[u8; N]>, t2: &mut Transport<[u8; N]>) {
//     fn step<const N: usize>(transport: &mut Transport<[u8; N]>, socket: (&mut [u8], &mut usize)) {
//         let mut scope = transport.scope();

//         scope.rx.feed(&socket.0[..*socket.1]).ok();
//         for _ in scope.rx.flush(&mut scope.state) {}

//         if let Some(bytes) = scope.tx.interact(&mut scope.state) {
//             socket.0[..bytes.len()].copy_from_slice(bytes);
//             *socket.1 = bytes.len();
//         }
//     }

//     let mut socket = [0u8; N];
//     let mut length = 0;

//     // Init one of them
//     if let Some(bytes) = t1.init() {
//         socket[..bytes.len()].copy_from_slice(bytes);
//         length = bytes.len();
//     } else if let Some(bytes) = t2.init() {
//         socket[..bytes.len()].copy_from_slice(bytes);
//         length = bytes.len();
//     }

//     // We may only need 2.5 but we do it 3 times anyway
//     for _ in 0..3 {
//         step(t1, (&mut socket, &mut length));
//         step(t2, (&mut socket, &mut length));
//     }
// }

// #[test]
// fn transport_handshake_non_streamed() {
//     let mut t1 = Transport::new([0u8; MAX_PAYLOAD_SIZE * NUM_ITER]).connect();
//     let mut t2 = Transport::new([0u8; MAX_PAYLOAD_SIZE * NUM_ITER]).listen();
//     dumb_handshake(&mut t1, &mut t2);
//     assert!(t1.opened() && t2.opened());

//     let mut t1 = Transport::new([0u8; MAX_PAYLOAD_SIZE * NUM_ITER]).listen();
//     let mut t2 = Transport::new([0u8; MAX_PAYLOAD_SIZE * NUM_ITER]).connect();
//     dumb_handshake(&mut t1, &mut t2);
//     assert!(t1.opened() && t2.opened());
// }

// #[test]
// fn transport_handshake_streamed() {
//     let mut t1 = Transport::new([0u8; MAX_PAYLOAD_SIZE * NUM_ITER])
//         .connect()
//         .streamed();
//     let mut t2 = Transport::new([0u8; MAX_PAYLOAD_SIZE * NUM_ITER])
//         .listen()
//         .streamed();
//     dumb_handshake(&mut t1, &mut t2);
//     assert!(t1.opened() && t2.opened());

//     let mut t1 = Transport::new([0u8; MAX_PAYLOAD_SIZE * NUM_ITER])
//         .listen()
//         .streamed();
//     let mut t2 = Transport::new([0u8; MAX_PAYLOAD_SIZE * NUM_ITER])
//         .connect()
//         .streamed();
//     dumb_handshake(&mut t1, &mut t2);
//     assert!(t1.opened() && t2.opened());
// }
