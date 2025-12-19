use crate::{exts::*, fields::*, msgs::*, *};

#[derive(Debug, PartialEq)]
pub enum Header<'a> {
    Owned(FrameHeader),
    Borrowed(&'a FrameHeader),
}

pub struct BatchReader<'a, T> {
    reader: T,
    _lt: core::marker::PhantomData<&'a ()>,
    frame: Option<FrameHeader>,
}

impl<'a, T> BatchReader<'a, T>
where
    T: crate::ZReadable<'a>,
{
    pub fn new(reader: T) -> Self {
        Self {
            reader,
            _lt: core::marker::PhantomData,
            frame: None,
        }
    }
}

impl<'a, T> Iterator for BatchReader<'a, T>
where
    T: crate::ZReadable<'a>,
{
    type Item = Message<'a>;

    fn next(&mut self) -> Option<Self::Item> {
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

        let reliability = self.frame.as_ref().map(|f| f.reliability);
        let qos = self.frame.as_ref().map(|f| f.qos);

        let body = match header & 0b0001_1111 {
            InitAck::ID if ack => Message::Transport(TransportMessage::InitAck(decode!(InitAck))),
            InitSyn::ID => Message::Transport(TransportMessage::InitSyn(decode!(InitSyn))),
            OpenAck::ID if ack => Message::Transport(TransportMessage::OpenAck(decode!(OpenAck))),
            OpenSyn::ID => Message::Transport(TransportMessage::OpenSyn(decode!(OpenSyn))),
            Close::ID => Message::Transport(TransportMessage::Close(decode!(Close))),
            KeepAlive::ID => Message::Transport(TransportMessage::KeepAlive(decode!(KeepAlive))),

            FrameHeader::ID => {
                let frame = decode!(FrameHeader);
                self.frame = Some(frame);
                return self.next();
            }
            Push::ID if net => Message::Network(NetworkMessage::Push {
                reliability: reliability.expect("Should be a frame. Something went wrong."),
                qos: qos.expect("Should be a frame. Something went wrong."),
                body: decode!(Push),
            }),
            Request::ID if net => Message::Network(NetworkMessage::Request {
                reliability: reliability.expect("Should be a frame. Something went wrong."),
                qos: qos.expect("Should be a frame. Something went wrong."),
                body: decode!(Request),
            }),
            Response::ID if net => Message::Network(NetworkMessage::Response {
                reliability: reliability.expect("Should be a frame. Something went wrong."),
                qos: qos.expect("Should be a frame. Something went wrong."),
                body: decode!(Response),
            }),
            ResponseFinal::ID if net => Message::Network(NetworkMessage::ResponseFinal {
                reliability: reliability.expect("Should be a frame. Something went wrong."),
                qos: qos.expect("Should be a frame. Something went wrong."),
                body: decode!(ResponseFinal),
            }),
            InterestFinal::ID if net && ifinal => Message::Network(NetworkMessage::InterestFinal {
                reliability: reliability.expect("Should be a frame. Something went wrong."),
                qos: qos.expect("Should be a frame. Something went wrong."),
                body: decode!(InterestFinal),
            }),
            Interest::ID if net => Message::Network(NetworkMessage::Interest {
                reliability: reliability.expect("Should be a frame. Something went wrong."),
                qos: qos.expect("Should be a frame. Something went wrong."),
                body: decode!(Interest),
            }),
            Declare::ID if net => Message::Network(NetworkMessage::Declare {
                reliability: reliability.expect("Should be a frame. Something went wrong."),
                qos: qos.expect("Should be a frame. Something went wrong."),
                body: decode!(Declare),
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
}

pub struct BatchWriter<'a, T> {
    writer: T,
    _lt: core::marker::PhantomData<&'a ()>,
    frame: Option<FrameHeader>,
    pub(crate) sn: u32,

    init: usize,
}

impl<'a, T> BatchWriter<'a, T>
where
    T: crate::ZWriteable,
{
    pub fn new(writer: T, sn: u32) -> Self {
        let init = writer.remaining();
        Self {
            writer,
            _lt: core::marker::PhantomData,
            frame: None,
            sn,
            init,
        }
    }

    pub fn has_written(&self) -> bool {
        self.init != self.writer.remaining()
    }

    pub fn finalize(self) -> (u32, usize) {
        (self.sn, self.init - self.writer.remaining())
    }
}

pub trait ZUnframed: ZEncode {}

impl ZUnframed for InitSyn<'_> {}
impl ZUnframed for InitAck<'_> {}
impl ZUnframed for OpenSyn<'_> {}
impl ZUnframed for OpenAck<'_> {}
impl ZUnframed for KeepAlive {}
impl ZUnframed for Close {}

impl<'a, W> BatchWriter<'a, W>
where
    W: crate::ZWriteable,
{
    pub fn unframed(&mut self, x: &impl ZUnframed) -> core::result::Result<(), crate::CodecError> {
        <_ as ZEncode>::z_encode(x, &mut self.writer)?;
        self.frame = None;
        Ok(())
    }
}

pub trait ZFramed: ZEncode {}

impl ZFramed for Push<'_> {}
impl ZFramed for Request<'_> {}
impl ZFramed for Response<'_> {}
impl ZFramed for ResponseFinal {}
impl ZFramed for Interest<'_> {}
impl ZFramed for Declare<'_> {}

impl<'a, W> BatchWriter<'a, W>
where
    W: crate::ZWriteable,
{
    pub fn framed(
        &mut self,
        x: &impl ZFramed,
        r: Reliability,
        qos: QoS,
    ) -> core::result::Result<(), crate::CodecError> {
        if self.frame.as_ref().map(|f| f.reliability) != Some(r) {
            <_ as ZEncode>::z_encode(
                &FrameHeader {
                    reliability: r,
                    sn: self.sn,
                    qos,
                },
                &mut self.writer,
            )?;

            self.frame = Some(FrameHeader {
                reliability: r,
                sn: self.sn,
                qos,
            });

            self.sn += 1;
        }

        <_ as ZEncode>::z_encode(x, &mut self.writer)?;

        Ok(())
    }
}

pub struct OneShotWriter<'a, T>(BatchWriter<'a, T>);

impl<'a, T> OneShotWriter<'a, T>
where
    T: crate::ZWriteable,
{
    pub fn new(writer: T) -> Self {
        Self(BatchWriter::new(writer, 0))
    }

    pub fn unframed(
        mut self,
        x: &impl ZUnframed,
    ) -> core::result::Result<(u32, usize), crate::CodecError> {
        self.0.unframed(x)?;

        Ok(self.0.finalize())
    }

    pub fn framed(
        mut self,
        x: &impl ZFramed,
        r: Reliability,
        qos: QoS,
        sn: u32,
    ) -> core::result::Result<(u32, usize), crate::CodecError> {
        self.0.sn = sn;
        self.0.framed(x, r, qos)?;

        Ok(self.0.finalize())
    }
}

pub struct AdvancingWriter<'a> {
    buffer: &'a mut [u8],
}

impl<'a> AdvancingWriter<'a> {
    pub fn new(buffer: &'a mut [u8]) -> Self {
        Self { buffer }
    }

    pub fn unframed(
        &mut self,
        x: &impl ZUnframed,
    ) -> core::result::Result<&'a mut [u8], crate::CodecError> {
        let len = OneShotWriter::new(&mut self.buffer[..]).unframed(x)?.1;

        let (written, remaining) = core::mem::take(&mut self.buffer).split_at_mut(len);
        self.buffer = remaining;

        Ok(written)
    }

    pub fn framed(
        &mut self,
        x: &impl ZFramed,
        r: Reliability,
        qos: QoS,
        sn: u32,
    ) -> core::result::Result<&'a mut [u8], crate::CodecError> {
        let len = OneShotWriter::new(&mut self.buffer[..])
            .framed(x, r, qos, sn)?
            .1;

        let (written, remaining) = core::mem::take(&mut self.buffer).split_at_mut(len);
        self.buffer = remaining;

        Ok(written)
    }
}
