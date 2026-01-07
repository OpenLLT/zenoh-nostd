use core::time::Duration;
use core::{fmt::Display, u16};

use crate::establishment::Description;
use crate::fields::BatchSize;
use crate::msgs::{InitIdentifier, InitResolution, InitSyn, TransportMessage};
use crate::{
    TransportError,
    fields::{Resolution, ZenohIdProto},
};

pub(crate) mod establishment;
pub(crate) mod helper;

mod rx;
mod tx;

mod handshake;

pub use rx::*;
pub use tx::*;

pub struct Transport<Buff> {
    zid: ZenohIdProto,
    streamed: bool,
    batch_size: u16,
    lease: Duration,
    resolution: Resolution,

    buff: Buff,
}

impl<Buff> Transport<Buff> {
    pub fn new(buff: Buff) -> Self
    where
        Buff: AsRef<[u8]>,
    {
        Transport {
            zid: ZenohIdProto::default(),
            streamed: false,
            batch_size: buff.as_ref().len() as u16,
            lease: Duration::from_secs(10),
            resolution: Resolution::default(),
            buff: buff,
        }
    }
    pub fn with_zid(mut self, zid: ZenohIdProto) -> Self {
        self.zid = zid;
        self
    }

    pub fn streamed(mut self) -> Self {
        self.streamed = true;
        self
    }

    pub fn with_batch_size(mut self, batch_size: u16) -> Self {
        self.batch_size = batch_size;
        self
    }

    pub fn with_lease(mut self, lease: Duration) -> Self {
        self.lease = lease;
        self
    }

    pub fn with_resolution(mut self, resolution: Resolution) -> Self {
        self.resolution = resolution;
        self
    }

    pub fn with_buff<NewBuff>(self, buff: NewBuff) -> Transport<NewBuff> {
        Transport {
            zid: self.zid,
            streamed: self.streamed,
            batch_size: self.batch_size,
            lease: self.lease,
            resolution: self.resolution,
            buff,
        }
    }

    pub fn codec(self) -> OpenedTransport<Buff>
    where
        Buff: Clone,
    {
        OpenedTransport {
            tx: TransportTx::new(
                self.buff.clone(),
                self.streamed,
                self.batch_size as usize,
                0,
                self.resolution,
                self.lease,
            ),
            rx: TransportRx::new(
                self.buff,
                self.streamed,
                self.batch_size as usize,
                0,
                self.resolution,
                self.lease,
            ),
            mine_zid: self.zid,
            other_zid: self.zid,
        }
    }

    pub fn listen<E>(
        mut self,
        read: impl FnMut(&mut [u8]) -> core::result::Result<usize, E>,
        write: impl FnMut(&[u8]) -> core::result::Result<(), E>,
    ) -> core::result::Result<OpenedTransport<Buff>, TransportError>
    where
        E: Display,
    {
        todo!()
    }

    pub fn connect<E>(
        mut self,
        mut write: impl FnMut(&[u8]) -> core::result::Result<(), E>,
    ) -> core::result::Result<OpenedTransport<Buff>, TransportError>
    where
        E: Display,
        Buff: AsMut<[u8]> + AsRef<[u8]>,
    {
        let init = InitSyn {
            identifier: InitIdentifier {
                zid: self.zid,
                ..Default::default()
            },
            resolution: InitResolution {
                resolution: self.resolution,
                batch_size: BatchSize(self.batch_size),
            },
            ..Default::default()
        };

        let buff_mut = self.buff.as_mut();

        let slice_mut = if self.streamed {
            if buff_mut.len() < 2 {
                crate::zbail!(@log TransportError::TransportTooSmall);
            }

            &mut buff_mut[2..]
        } else {
            &mut buff_mut[..]
        };

        let len = crate::codec::transport_encoder(
            slice_mut,
            core::iter::once(TransportMessage::InitSyn(init)),
        )
        .sum::<usize>();

        let len = if self.streamed {
            let length = (len as u16).to_le_bytes();
            buff_mut[..2].copy_from_slice(&length);
            len + 2
        } else {
            len
        };

        write(&buff_mut[..len]).map_err(|e| {
            crate::error!("{e}");
            TransportError::CouldNotWrite
        })?;

        todo!()
    }
}

pub struct OpenedTransport<Buff> {
    pub tx: TransportTx<Buff>,
    pub rx: TransportRx<Buff>,

    pub mine_zid: ZenohIdProto,
    pub other_zid: ZenohIdProto,
}

impl<Buff> From<(Description, bool, Buff, Buff)> for OpenedTransport<Buff> {
    fn from(value: (Description, bool, Buff, Buff)) -> Self {
        let (description, streamed, tx, rx) = value;

        OpenedTransport {
            tx: TransportTx::new(
                tx,
                streamed,
                description.batch_size as usize,
                description.mine_sn,
                description.resolution,
                description.mine_lease,
            ),
            rx: TransportRx::new(
                rx,
                streamed,
                description.batch_size as usize,
                description.other_sn,
                description.resolution,
                description.other_lease,
            ),
            mine_zid: description.mine_zid,
            other_zid: description.other_zid,
        }
    }
}

impl<Buff> OpenedTransport<Buff> {
    pub(crate) fn new(description: Description, streamed: bool, tx: Buff, rx: Buff) -> Self {
        Self {
            tx: TransportTx::new(
                tx,
                streamed,
                description.batch_size as usize,
                description.mine_sn,
                description.resolution,
                description.mine_lease,
            ),
            rx: TransportRx::new(
                rx,
                streamed,
                description.batch_size as usize,
                description.other_sn,
                description.resolution,
                description.other_lease,
            ),
            mine_zid: description.mine_zid,
            other_zid: description.other_zid,
        }
    }

    pub fn sync(&mut self, _: core::time::Duration) {}
}
