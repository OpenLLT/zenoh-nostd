use core::time::Duration;

use heapless::Entry;
use zenoh_proto::{
    AdvancingWriter, BatchReader, OneShotWriter,
    fields::{BatchSize, Resolution, ZenohIdProto},
    msgs::*,
    zerror::CollectionError,
};

use crate::PacketWithDst;

mod handle;

#[derive(Debug)]
enum State {
    NotInitialized,
    Initialized {
        zid: ZenohIdProto,
        batch_size: u16,
        resolution: Resolution,
        sn: u32,
    },
    Opened {
        zid: ZenohIdProto,
        batch_size: u16,
        resolution: Resolution,
        sn: u32,
        lease: Duration,
    },
}

pub struct Broker<const N: usize> {
    buffer: [u8; N],

    zid: ZenohIdProto,
    lease: Duration,

    conn: heapless::FnvIndexMap<Id, State, 16>,
}

#[derive(Debug, PartialEq, Clone, Copy, Hash, Eq)]
pub enum Id {
    North,
    South(u32),
}

impl<const N: usize> Broker<N> {
    pub fn new() -> Self {
        Self {
            buffer: [0; N],
            zid: ZenohIdProto::default(),
            lease: Duration::from_secs(10),
            conn: heapless::FnvIndexMap::from_iter(core::iter::once((
                Id::North,
                State::NotInitialized,
            ))),
        }
    }

    pub fn init(&mut self) -> core::result::Result<&[u8], zenoh_proto::zerror::CodecError> {
        let w = &mut self.buffer[..];
        let len = OneShotWriter::new(w)
            .unframed(&InitSyn {
                identifier: InitIdentifier {
                    zid: self.zid.clone(),
                    ..Default::default()
                },
                resolution: InitResolution {
                    batch_size: BatchSize(N as u16),
                    ..Default::default()
                },
                ..Default::default()
            })?
            .1;
        Ok(&self.buffer[..len])
    }

    pub fn lease(&mut self, now: Duration) -> (Duration, impl Iterator<Item = PacketWithDst<'_>>) {
        (self.lease / 3, core::iter::empty())
    }

    pub fn recv<'s, 'a, const P: usize>(
        &'s mut self,
        data: &'a [u8],
        id: Id,
    ) -> crate::ZResult<impl Iterator<Item = PacketWithDst<'s>>> {
        let Self {
            buffer,
            zid,
            lease,
            conn,
        } = self;

        let entry = match conn.entry(id) {
            Entry::Occupied(n) => n.into_mut(),
            Entry::Vacant(v) => v
                .insert(State::NotInitialized)
                .map_err(|_| CollectionError::CollectionIsFull)?,
        };

        let mut w = AdvancingWriter::new(&mut buffer[..]);

        let mut packets = heapless::Vec::<PacketWithDst, P>::new();
        let batch = BatchReader::new(data);

        for msg in batch {
            match msg {
                Message::Transport(TransportMessage::InitAck(ack)) => {
                    handle::init_ack::<N, P>(zid, *lease, entry, ack, id, &mut w, &mut packets)?
                }
                Message::Transport(TransportMessage::OpenAck(ack)) => {
                    handle::open_ack(entry, ack, id)?
                }
                _ => {}
            }
        }

        Ok(packets.into_iter())
    }
}
