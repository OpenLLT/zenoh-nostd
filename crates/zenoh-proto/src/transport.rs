use crate::{
    ZEncode, ZReadable,
    exts::QoS,
    fields::Reliability,
    msgs::{
        Close, Declare, FrameHeader, InitAck, InitSyn, Interest, InterestFinal, KeepAlive, Message,
        MessageIter, NetworkBody, NetworkMessage, OpenAck, OpenSyn, Push, Request, Response,
        ResponseFinal, TransportMessage,
    },
};

pub struct TransportReceiver<'a> {
    state: &'a mut TransportState,
    reader: &'a [u8],
    frame: Option<FrameHeader>,
}

struct Mark<'a> {
    ptr: *const u8,
    len: usize,
    _lt: core::marker::PhantomData<&'a ()>,
}

impl<'a> TransportReceiver<'a> {
    pub(crate) fn new(state: &'a mut TransportState, reader: &'a [u8]) -> Self {
        Self {
            state,
            reader,
            frame: None,
        }
    }

    fn mark(&self) -> Mark<'a> {
        Mark {
            ptr: self.reader.as_ptr(),
            len: self.reader.len(),
            _lt: core::marker::PhantomData,
        }
    }

    fn rewind(&mut self, mark: Mark<'a>) {
        unsafe {
            self.reader = core::slice::from_raw_parts(mark.ptr, mark.len);
        }
    }

    fn decode(&mut self) -> Option<Message<'a>> {
        if !self.reader.can_read() {
            return None;
        }

        let header = self
            .reader
            .read_u8()
            .expect("reader should not be empty at this stage");

        macro_rules! decode {
            ($ty:ty) => {
                match <$ty as $crate::ZBodyDecode>::z_body_decode(&mut self.reader, header) {
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
        let net = self.frame.is_some();
        let ifinal = header & 0b0110_0000 == 0;
        let id = header & 0b0001_1111;

        if let Some(tmsg) = match id {
            InitAck::ID if ack => Some(TransportMessage::InitAck(decode!(InitAck))),
            InitSyn::ID => Some(TransportMessage::InitSyn(decode!(InitSyn))),
            OpenAck::ID if ack => Some(TransportMessage::OpenAck(decode!(OpenAck))),
            OpenSyn::ID => Some(TransportMessage::OpenSyn(decode!(OpenSyn))),
            Close::ID => Some(TransportMessage::Close(decode!(Close))),
            KeepAlive::ID => Some(TransportMessage::KeepAlive(decode!(KeepAlive))),
            _ => None,
        } {
            return self.state.handle_msg(tmsg).map(Message::Transport);
        }

        let reliability = self.frame.as_ref().map(|f| f.reliability);
        let qos = self.frame.as_ref().map(|f| f.qos);

        // TODO! negociate `sn` and expect increasing `sn` in frames

        let body = match id {
            FrameHeader::ID => {
                let frame = decode!(FrameHeader);
                self.frame = Some(frame);
                return self.decode();
            }
            Push::ID if net => NetworkBody::Push(decode!(Push)),
            Request::ID if net => NetworkBody::Request(decode!(Request)),
            Response::ID if net => NetworkBody::Response(decode!(Response)),
            ResponseFinal::ID if net => NetworkBody::ResponseFinal(decode!(ResponseFinal)),
            InterestFinal::ID if net && ifinal => {
                NetworkBody::InterestFinal(decode!(InterestFinal))
            }
            Interest::ID if net => NetworkBody::Interest(decode!(Interest)),
            Declare::ID if net => NetworkBody::Declare(decode!(Declare)),
            _ => {
                crate::error!(
                    "Unrecognized message header: {:08b}. Skipping the rest of the message - {}",
                    header,
                    crate::zctx!()
                );
                return None;
            }
        };

        Some(Message::Network(NetworkMessage {
            reliability: reliability.expect("Should be a frame. Something went wrong."),
            qos: qos.expect("Should be a frame. Something went wrong."),
            body,
        }))
    }

    pub fn next(
        &mut self,
    ) -> Option<
        MessageIter<
            'a,
            impl Iterator<Item = TransportMessage<'a>>,
            impl Iterator<Item = NetworkMessage<'a>>,
        >,
    > {
        let message = self.decode()?;

        match message {
            Message::Transport(message) => Some(MessageIter::Transport(
                core::iter::once(message).chain(core::iter::from_fn(move || {
                    let mark = self.mark();
                    match self.decode()? {
                        Message::Transport(msg) => Some(msg),
                        _ => {
                            self.rewind(mark);
                            None
                        }
                    }
                })),
            )),
            Message::Network(message) => Some(MessageIter::Network(
                core::iter::once(message).chain(core::iter::from_fn(move || {
                    let mark = self.mark();
                    match self.decode()? {
                        Message::Network(msg) => Some(msg),
                        _ => {
                            self.rewind(mark);
                            None
                        }
                    }
                })),
            )),
        }
    }
}

pub struct TransportSender<Tx> {
    streamed: bool,

    tx: Tx,
    batch_size: u16,

    sn: u32,
    reliability: Option<Reliability>,
    qos: Option<QoS>,
}

impl<Tx> TransportSender<Tx> {
    pub fn send<'s, 'm>(
        &'s mut self,
        transport: impl Iterator<Item = TransportMessage<'m>>,
        network: impl Iterator<Item = NetworkMessage<'m>>,
    ) -> impl Iterator<Item = &'s [u8]>
    where
        Tx: AsMut<[u8]> + 's,
    {
        let streamed = self.streamed;
        let mut buffer = self.tx.as_mut();
        let batch_size = core::cmp::min(self.batch_size as usize, buffer.len());

        let mut transport = transport.peekable();
        let mut network = network.peekable();

        let reliability = &mut self.reliability;
        let qos = &mut self.qos;
        let sn = &mut self.sn;

        core::iter::from_fn(move || {
            let batch_size = core::cmp::min(batch_size as usize, buffer.len());
            let batch = &mut buffer[..batch_size];

            if streamed && batch_size < 2 {
                return None;
            }

            let mut writer = &mut batch[if streamed { 2 } else { 0 }..];
            let start = writer.len();

            let mut length = 0;
            extern crate std;
            std::println!("Encoding transport messages...");

            'moop: loop {
                while let Some(msg) = transport.peek() {
                    std::println!("Encoding transport message: {:?}", msg);

                    if msg.z_encode(&mut writer).is_ok() {
                        std::println!("Encoded transport message successfully.");
                        length = start - writer.len();
                        transport.next();
                    } else {
                        std::println!("Failed to encode transport message, breaking.");
                        break 'moop;
                    }
                }

                std::println!("Encoding network messages...");
                while let Some(msg) = network.peek() {
                    if msg.z_encode(&mut writer, reliability, qos, sn).is_ok() {
                        length = start - writer.len();
                        network.next();
                    } else {
                        break;
                    }
                }

                break 'moop;
            }

            if length == 0 {
                return None;
            }

            if streamed {
                let l = (length as u16).to_be_bytes();
                batch[..2].copy_from_slice(&l);
            }

            let (ret, remain) =
                core::mem::take(&mut buffer).split_at_mut(length + if streamed { 2 } else { 0 });
            buffer = remain;

            Some(&ret[..])
        })
    }
}

#[derive(Default)]
pub enum TransportState {
    #[default]
    EncodeDecode,

    Uninitialized,
    Initialized,
    Opened,
}

impl TransportState {
    fn handle_msg<'a>(&mut self, msg: TransportMessage<'a>) -> Option<TransportMessage<'a>> {
        if matches!(self, TransportState::EncodeDecode) {
            return Some(msg);
        }

        None
    }
}

pub struct Transport<Tx, Rx> {
    state: TransportState,
    streamed: bool,

    rx: Rx,
    sender: TransportSender<Tx>,
}

impl<Tx, Rx> Transport<Tx, Rx> {
    pub fn new(streamed: bool, tx: Tx, rx: Rx, batch_size: u16, sn: u32) -> Self {
        Self {
            state: TransportState::EncodeDecode,
            streamed,
            rx,
            sender: TransportSender {
                streamed,
                tx,
                batch_size,
                sn,
                reliability: None,
                qos: None,
            },
        }
    }

    pub fn update(
        &mut self,
        mut read: impl FnMut(&mut [u8]) -> usize,
    ) -> (TransportReceiver<'_>, &mut TransportSender<Tx>)
    where
        Rx: AsMut<[u8]>,
    {
        let Self {
            state,
            streamed,
            rx,
            sender,
        } = self;

        let rx = if *streamed {
            let mut lbuf = [0u8; 2];
            read(&mut lbuf);
            let l = u16::from_be_bytes(lbuf) as usize;
            read(&mut rx.as_mut()[..l]);
            &rx.as_mut()[..l]
        } else {
            let buf = rx.as_mut();
            let l = read(buf);
            &buf[..l]
        };

        (TransportReceiver::new(state, rx), sender)
    }

    pub fn tx(&mut self) -> &mut TransportSender<Tx> {
        &mut self.sender
    }
}
