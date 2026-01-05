pub mod ext;
pub mod r#struct;

pub use ext::*;
pub use r#struct::*;

use crate::{ZReadable, exts::*, fields::*, msgs::*};

fn decode<'a>(reader: &mut &'a [u8], last_frame: &mut Option<FrameHeader>) -> Option<Message<'a>> {
    if !reader.can_read() {
        return None;
    }

    let header = reader
        .read_u8()
        .expect("reader should not be empty at this stage");

    macro_rules! decode {
        ($ty:ty) => {
            match <$ty as $crate::ZBodyDecode>::z_body_decode(reader, header) {
                Ok(msg) => msg,
                Err(e) => {
                    crate::error!(
                        "Failed to decode message of type {}: {}. Skipping the rest of the message - {}",
                        core::any::type_name::<$ty>(),
                        e,
                        crate::zctx!()
                    );

                    return None;
                }
            }
        };
    }

    let ack = header & 0b0010_0000 != 0;
    let net = last_frame.is_some();
    let ifinal = header & 0b0110_0000 == 0;
    let id = header & 0b0001_1111;

    let reliability = last_frame.as_ref().map(|f| f.reliability);
    let qos = last_frame.as_ref().map(|f| f.qos);
    let sn = last_frame.as_ref().map(|f| f.sn);

    let body = match id {
        FrameHeader::ID => {
            let header = decode!(FrameHeader);

            if let Some(sn) = sn {
                if header.sn <= sn && sn != 0 {
                    // Special case when 0
                    crate::error!(
                        "Inconsistent `SN` value {}, expected higher than {}",
                        header.sn,
                        sn
                    );
                    return None;
                } else if header.sn != sn + 1 {
                    crate::debug!("Transport missed {} messages", header.sn - sn - 1);
                }
            }

            last_frame.replace(header);
            return decode(reader, last_frame);
        }
        InitAck::ID if ack => Message::Transport(TransportMessage::InitAck(decode!(InitAck))),
        InitSyn::ID => Message::Transport(TransportMessage::InitSyn(decode!(InitSyn))),
        OpenAck::ID if ack => Message::Transport(TransportMessage::OpenAck(decode!(OpenAck))),
        OpenSyn::ID => Message::Transport(TransportMessage::OpenSyn(decode!(OpenSyn))),
        Close::ID => Message::Transport(TransportMessage::Close(decode!(Close))),
        KeepAlive::ID => Message::Transport(TransportMessage::KeepAlive(decode!(KeepAlive))),
        Push::ID if net => Message::Network(NetworkMessage {
            reliability: reliability.expect("Should be a frame. Something went wrong."),
            qos: qos.expect("Should be a frame. Something went wrong."),
            body: NetworkBody::Push(decode!(Push)),
        }),
        Request::ID if net => Message::Network(NetworkMessage {
            reliability: reliability.expect("Should be a frame. Something went wrong."),
            qos: qos.expect("Should be a frame. Something went wrong."),
            body: NetworkBody::Request(decode!(Request)),
        }),
        Response::ID if net => Message::Network(NetworkMessage {
            reliability: reliability.expect("Should be a frame. Something went wrong."),
            qos: qos.expect("Should be a frame. Something went wrong."),
            body: NetworkBody::Response(decode!(Response)),
        }),
        ResponseFinal::ID if net => Message::Network(NetworkMessage {
            reliability: reliability.expect("Should be a frame. Something went wrong."),
            qos: qos.expect("Should be a frame. Something went wrong."),
            body: NetworkBody::ResponseFinal(decode!(ResponseFinal)),
        }),
        InterestFinal::ID if net && ifinal => Message::Network(NetworkMessage {
            reliability: reliability.expect("Should be a frame. Something went wrong."),
            qos: qos.expect("Should be a frame. Something went wrong."),
            body: NetworkBody::InterestFinal(decode!(InterestFinal)),
        }),
        Interest::ID if net => Message::Network(NetworkMessage {
            reliability: reliability.expect("Should be a frame. Something went wrong."),
            qos: qos.expect("Should be a frame. Something went wrong."),
            body: NetworkBody::Interest(decode!(Interest)),
        }),
        Declare::ID if net => Message::Network(NetworkMessage {
            reliability: reliability.expect("Should be a frame. Something went wrong."),
            qos: qos.expect("Should be a frame. Something went wrong."),
            body: NetworkBody::Declare(decode!(Declare)),
        }),
        _ => {
            crate::error!(
                "Unrecognized message header: {:08b}. Skipping the rest of the message - {}",
                header,
                crate::zctx!()
            );
            return None;
        }
    };

    Some(body)
}

pub(crate) fn decoder<'a>(bytes: &'a [u8]) -> impl Iterator<Item = (Message<'a>, &'a [u8])> {
    let mut reader = &bytes[..];
    let mut last_frame: Option<FrameHeader> = None;

    core::iter::from_fn(move || {
        let (data, start) = (reader.as_ptr(), reader.len());
        let msg = decode(&mut reader, &mut last_frame);
        let len = start - reader.len();
        msg.map(|msg| (msg, unsafe { core::slice::from_raw_parts(data, len) }))
    })
}

pub(crate) fn transport_decoder<'a>(
    bytes: &'a [u8],
) -> impl Iterator<Item = (TransportMessage<'a>, &'a [u8])> {
    decoder(bytes).filter_map(|m| match m.0 {
        Message::Transport(msg) => Some((msg, m.1)),
        _ => None,
    })
}

pub(crate) fn network_decoder<'a>(
    bytes: &'a [u8],
) -> impl Iterator<Item = (NetworkMessage<'a>, &'a [u8])> {
    decoder(bytes).filter_map(|m| match m.0 {
        Message::Network(msg) => Some((msg, m.1)),
        _ => None,
    })
}

fn encode<'a, 'b>(
    writer: &mut &'a mut [u8],
    msg: Message<'b>,
    reliability: &mut Option<Reliability>,
    qos: &mut Option<QoS>,
    next_sn: &mut u32,
) -> Option<usize> {
    let start = writer.len();

    match msg {
        Message::Network(msg) => {
            let r = msg.reliability;
            let q = msg.qos;

            if reliability.as_ref() != Some(&r) || qos.as_ref() != Some(&q) {
                FrameHeader {
                    reliability: r,
                    sn: *next_sn,
                    qos: q,
                }
                .z_encode(writer)
                .ok()?;

                *reliability = Some(r);
                *qos = Some(q);
                *next_sn = next_sn.wrapping_add(1);
            }

            msg.body.z_encode(writer).ok()
        }
        Message::Transport(msg) => msg.z_encode(writer).ok(),
    }?;

    Some(start - writer.len())
}

pub(crate) fn encoder<'a, 'b>(
    bytes: &'a mut [u8],
    mut msgs: impl Iterator<Item = Message<'b>>,
    next_sn: &mut u32,
) -> impl Iterator<Item = usize> {
    let mut writer = &mut bytes[..];
    let mut last_reliability: Option<Reliability> = None;
    let mut last_qos: Option<QoS> = None;
    core::iter::from_fn(move || {
        let msg = msgs.next()?;
        encode(
            &mut writer,
            msg,
            &mut last_reliability,
            &mut last_qos,
            next_sn,
        )
    })
}

pub(crate) fn transport_encoder<'a, 'b>(
    bytes: &'a mut [u8],
    mut msgs: impl Iterator<Item = TransportMessage<'b>>,
) -> impl Iterator<Item = usize> {
    let mut writer = &mut bytes[..];
    core::iter::from_fn(move || {
        let msg = msgs.next()?;
        encode(
            &mut writer,
            Message::Transport(msg),
            &mut None,
            &mut None,
            &mut 0,
        )
    })
}

pub(crate) fn network_encoder<'a, 'b>(
    bytes: &'a mut [u8],
    mut msgs: impl Iterator<Item = NetworkMessage<'b>>,
    next_sn: &mut u32,
) -> impl Iterator<Item = usize> {
    let mut writer = &mut bytes[..];
    let mut last_reliability: Option<Reliability> = None;
    let mut last_qos: Option<QoS> = None;
    core::iter::from_fn(move || {
        let msg = msgs.next()?;
        encode(
            &mut writer,
            Message::Network(msg),
            &mut last_reliability,
            &mut last_qos,
            next_sn,
        )
    })
}
