use rand::{Rng, thread_rng};

use crate::{exts::*, fields::*, msgs::*, *};

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

#[test]
fn transport_codec() {
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

    let mut socket = [0u8; MAX_PAYLOAD_SIZE * NUM_ITER];
    let mut writer = &mut socket[..];

    let mut transport = Transport::new([0u8; MAX_PAYLOAD_SIZE * NUM_ITER]).codec();
    let mut scope = transport.scope();

    for chunk in scope.tx.write(messages.clone().into_iter()) {
        writer[..chunk.len()].copy_from_slice(chunk);
        let (_, remain) = writer.split_at_mut(chunk.len());
        writer = remain;
    }

    let len = MAX_PAYLOAD_SIZE * NUM_ITER - writer.len();
    scope.rx.feed(&socket[..len]);

    for msg in scope.rx.flush(&mut scope.state) {
        let actual = messages.pop_front().unwrap();
        assert_eq!(msg, actual);
    }

    assert!(messages.is_empty());
}

#[test]
fn transport_handshake() {
    let mut t1 = Transport::new([0u8; MAX_PAYLOAD_SIZE * NUM_ITER]).connect();
    let mut t2 = Transport::new([0u8; MAX_PAYLOAD_SIZE * NUM_ITER]).listen();

    fn step<const N: usize>(transport: &mut Transport<[u8; N]>, socket: (&mut [u8], &mut usize)) {
        let mut scope = transport.scope();

        scope.rx.feed(&socket.0[..*socket.1]);
        for _ in scope.rx.flush(&mut scope.state) {}

        if let Some(i) = scope.tx.interact(&mut scope.state) {
            socket.0[..i.len()].copy_from_slice(i);
            *socket.1 = i.len();
        }
    }

    let mut socket = [0u8; MAX_PAYLOAD_SIZE * NUM_ITER];
    let mut length = 0;
    for _ in 0..2 {
        step(&mut t1, (&mut socket, &mut length));
        step(&mut t2, (&mut socket, &mut length));
    }

    step(&mut t1, (&mut socket, &mut length));

    assert!(t1.opened() && t2.opened());

    let mut t1 = Transport::new([0u8; MAX_PAYLOAD_SIZE * NUM_ITER]).listen();
    let mut t2 = Transport::new([0u8; MAX_PAYLOAD_SIZE * NUM_ITER]).connect();

    let mut socket = [0u8; MAX_PAYLOAD_SIZE * NUM_ITER];
    let mut length = 0;
    for _ in 0..3 {
        step(&mut t1, (&mut socket, &mut length));
        step(&mut t2, (&mut socket, &mut length));
    }

    step(&mut t1, (&mut socket, &mut length));

    assert!(t1.opened() && t2.opened());
}
