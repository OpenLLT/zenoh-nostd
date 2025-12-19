use rand::{Rng, thread_rng};

use crate::{exts::*, fields::*, msgs::*};

macro_rules! roundtrip {
    ($ty:ty) => {{
        let mut rand = [0u8; MAX_PAYLOAD_SIZE];
        let mut data = [0u8; MAX_PAYLOAD_SIZE];

        for _ in 0..NUM_ITER {
            let value = <$ty>::rand(&mut &mut rand[..]);

            let len = $crate::ZLen::z_len(&value);
            $crate::ZEncode::z_encode(&value, &mut &mut data[..]).unwrap();

            let ret = <$ty as $crate::ZDecode>::z_decode(&mut &data[..len]).unwrap();

            assert_eq!(ret, value);
        }

        #[cfg(feature = "alloc")]
        {
            // Because random data generation uses the `ZStoreable` unsafe trait, we need
            // to avoid reallocation during the test to keep pointers valid.
            let mut rand = alloc::vec::Vec::with_capacity(MAX_PAYLOAD_SIZE);
            let mut data = alloc::vec::Vec::new();

            for _ in 0..NUM_ITER {
                rand.clear();
                data.clear();

                let value = <$ty>::rand(&mut rand);

                $crate::ZEncode::z_encode(&value, &mut data).unwrap();

                let ret = <$ty as $crate::ZDecode>::z_decode(&mut &data[..]).unwrap();

                assert_eq!(ret, value);
            }
        }
    }};

    (ext, $ty:ty) => {{
        let mut rand = [0u8; MAX_PAYLOAD_SIZE];
        let mut data = [0u8; MAX_PAYLOAD_SIZE];

        for _ in 0..NUM_ITER {
            let value = <$ty>::rand(&mut &mut rand[..]);

            $crate::zext_encode::<_, 0x1, true>(&value, &mut &mut data[..], false).unwrap();

            let ret = $crate::zext_decode::<$ty>(&mut &data[..]).unwrap();

            assert_eq!(ret, value);
        }

        #[cfg(feature = "alloc")]
        {
            // Because random data generation uses the `ZStoreable` unsafe trait, we need
            // to avoid reallocation during the test to keep pointers valid.
            let mut rand = alloc::vec::Vec::with_capacity(MAX_PAYLOAD_SIZE);
            let mut data = alloc::vec::Vec::new();

            for _ in 0..NUM_ITER {
                rand.clear();
                data.clear();

                let value = <$ty>::rand(&mut rand);

                $crate::zext_encode::<_, 0x1, true>(&value, &mut data, false).unwrap();

                let ret = $crate::zext_decode::<$ty>(&mut &data[..]).unwrap();

                assert_eq!(ret, value);
            }
        }
    }};
}

macro_rules! roundtrips {
    (ext, $namespace:ident, $($ty:ty),* $(,)?) => {
        $(
            paste::paste! {
                #[test]
                fn [<$namespace _proto_ext_ $ty:lower>]() {
                    roundtrip!(ext, $ty);
                }
            }
        )*
    };

    ($namespace:ident, $($ty:ty),* $(,)?) => {
        $(
            paste::paste! {
                #[test]
                fn [<$namespace _proto_ $ty:lower>]() {
                    roundtrip!($ty);
                }
            }
        )*
    };
}

const NUM_ITER: usize = 100;
const MAX_PAYLOAD_SIZE: usize = 512;

roundtrips!(ext, zenoh, EntityGlobalId, SourceInfo, Value, Attachment);
roundtrips!(zenoh, Err, Put, Query, Reply,);

roundtrips!(
    ext,
    network,
    QoS,
    NodeId,
    QueryTarget,
    Budget,
    QueryableInfo
);

roundtrips!(
    network,
    DeclareKeyExpr,
    UndeclareKeyExpr,
    DeclareSubscriber,
    UndeclareSubscriber,
    DeclareQueryable,
    UndeclareQueryable,
    DeclareToken,
    UndeclareToken,
    DeclareFinal,
    Declare,
    Interest,
    InterestFinal,
    Push,
    Request,
    Response,
    ResponseFinal,
);

roundtrips!(ext, transport, Auth, Patch);
roundtrips!(
    transport,
    Close,
    FrameHeader,
    InitSyn,
    InitAck,
    KeepAlive,
    OpenSyn,
    OpenAck
);

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

fn trans_rand<'a>(w: &mut impl crate::ZStoreable<'a>) -> TransportMessage<'a> {
    use rand::seq::SliceRandom;
    let mut rng = rand::thread_rng();
    let choices = [InitSyn::ID, OpenSyn::ID, Close::ID, KeepAlive::ID];

    match *choices.choose(&mut rng).unwrap() {
        InitSyn::ID => {
            if rng.gen_bool(0.5) {
                TransportMessage::InitSyn(InitSyn::rand(w))
            } else {
                TransportMessage::InitAck(InitAck::rand(w))
            }
        }
        OpenSyn::ID => {
            if rng.gen_bool(0.5) {
                TransportMessage::OpenSyn(OpenSyn::rand(w))
            } else {
                TransportMessage::OpenAck(OpenAck::rand(w))
            }
        }
        Close::ID => TransportMessage::Close(Close::rand(w)),
        KeepAlive::ID => TransportMessage::KeepAlive(KeepAlive::rand(w)),
        _ => unreachable!(),
    }
}

#[test]
fn network_stream() {
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

    let mut transport = crate::Transport::new(
        false,
        [0u8; MAX_PAYLOAD_SIZE * NUM_ITER],
        [0u8; MAX_PAYLOAD_SIZE * NUM_ITER],
        (MAX_PAYLOAD_SIZE * NUM_ITER) as u16,
        0,
    );

    let mut data = [0u8; MAX_PAYLOAD_SIZE * NUM_ITER];

    let slice = transport
        .tx()
        .send(core::iter::empty(), messages.clone().into_iter())
        .next()
        .expect("Should have at least one batch");

    let len = slice.len();
    data[..len].copy_from_slice(slice);

    let (mut rx, _) = transport.update(|d| {
        d[..len].copy_from_slice(&data[..len]);
        len
    });

    let batch = match rx.next() {
        Some(crate::msgs::MessageIter::Network(msgs)) => msgs,
        _ => panic!("Expected Network messages"),
    };

    for msg in batch {
        let actual = messages.pop_front().unwrap();
        assert_eq!(msg, actual);
    }

    assert!(messages.is_empty());
}

#[test]
fn transport_stream() {
    extern crate std;
    use std::collections::VecDeque;

    let mut rand = [0u8; MAX_PAYLOAD_SIZE * NUM_ITER];
    let mut rw = rand.as_mut_slice();

    let mut messages = {
        let mut msgs = VecDeque::new();
        for _ in 0..thread_rng().gen_range(1..16) {
            msgs.push_back(trans_rand(&mut rw));
        }
        msgs
    };

    let mut transport = crate::Transport::new(
        false,
        [0u8; MAX_PAYLOAD_SIZE * NUM_ITER],
        [0u8; MAX_PAYLOAD_SIZE * NUM_ITER],
        (MAX_PAYLOAD_SIZE * NUM_ITER) as u16,
        0,
    );

    let mut data = [0u8; MAX_PAYLOAD_SIZE * NUM_ITER];

    let slice = transport
        .tx()
        .send(messages.clone().into_iter(), core::iter::empty())
        .next()
        .expect("Should have at least one batch");

    let len = slice.len();
    data[..len].copy_from_slice(slice);

    let (mut rx, _) = transport.update(|d| {
        d[..len].copy_from_slice(&data[..len]);
        len
    });

    let batch = match rx.next() {
        Some(crate::msgs::MessageIter::Transport(msgs)) => msgs,
        _ => panic!("Expected Transport messages"),
    };

    for msg in batch {
        let actual = messages.pop_front().unwrap();
        assert_eq!(msg, actual);
    }

    assert!(messages.is_empty());
}

// #[test]
// fn transport_stream() {
//     extern crate std;
//     use std::collections::VecDeque;

//     let mut rand = [0u8; MAX_PAYLOAD_SIZE * NUM_ITER];
//     let mut rw = rand.as_mut_slice();

//     let mut messages = {
//         let mut msgs = VecDeque::new();
//         for _ in 1..thread_rng().gen_range(1..16) {
//             msgs.push_back((Reliability::rand(&mut rw), FrameBody::rand(&mut rw)));
//         }
//         msgs
//     };

//     let mut data = [0u8; MAX_PAYLOAD_SIZE * NUM_ITER];
//     let mut batch = BatchWriter::new(&mut data[..], 0);

//     for (r, msg) in &messages {
//         batch.framed(msg, *r, QoS::default()).unwrap();
//     }

//     batch.unframed(&KeepAlive {}).unwrap();

//     let (_, len) = batch.finalize();
//     let batch = BatchReader::new(&data[..len]);

//     let mut got_keepalive = false;
//     for msg in batch {
//         if let Some((_, actual)) = messages.pop_front() {
//             if actual.is(&msg) {
//                 continue;
//             } else {
//                 panic!("Frame message did not match");
//             }
//         }

//         match msg {
//             Message::Transport(TransportMessage::KeepAlive(_)) => {
//                 got_keepalive = true;
//             }
//             _ => panic!("First messages should be Frames, and last a KeepAlive"),
//         }
//     }

//     assert!(messages.is_empty());
//     assert!(got_keepalive);
// }
