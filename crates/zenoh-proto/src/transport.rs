use core::time::Duration;
use core::{fmt::Display, u16};

use crate::{
    TransportError,
    fields::{Resolution, ZenohIdProto},
    transport::{rx::TransportRx, tx::TransportTx},
};

mod establishment;

mod rx;
mod tx;

pub struct Transport<Buff> {
    zid: ZenohIdProto,
    batch_size: u16,
    lease: Duration,
    resolution: Resolution,

    buff: Buff,
}

impl Default for Transport<[u8; u16::MAX as usize]> {
    fn default() -> Self {
        Self {
            zid: ZenohIdProto::default(),
            batch_size: u16::MAX,
            lease: Duration::from_secs(10),
            resolution: Resolution::default(),
            buff: [0u8; u16::MAX as usize],
        }
    }
}

#[repr(u8)]
pub enum HandshakeError {
    CouldNotRead = 0,
    CouldNotWrite = 1,
}

impl<Buff> Transport<Buff> {
    pub fn with_zid(mut self, zid: ZenohIdProto) -> Self {
        self.zid = zid;
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
                self.batch_size as usize,
                0,
                self.resolution,
                self.lease,
            ),
            rx: TransportRx::new(
                self.buff,
                self.batch_size as usize,
                0,
                self.resolution,
                self.lease,
            ),
        }
    }

    pub fn sync_listen<E>(
        mut self,
        read: impl FnMut(&mut [u8]) -> core::result::Result<usize, E>,
        write: impl FnMut(&[u8]) -> core::result::Result<(), E>,
    ) -> core::result::Result<OpenedTransport<Buff>, TransportError>
    where
        E: Display,
    {
        todo!()
    }

    pub fn sync_connect<E>(
        mut self,
        read: impl FnMut(&mut [u8]) -> core::result::Result<usize, E>,
        write: impl FnMut(&[u8]) -> core::result::Result<(), E>,
    ) -> core::result::Result<OpenedTransport<Buff>, TransportError>
    where
        E: Display,
    {
        todo!()
    }

    pub async fn async_listen<E>(
        mut self,
        read: impl AsyncFnMut(&mut [u8]) -> core::result::Result<usize, E>,
        write: impl AsyncFnMut(&[u8]) -> core::result::Result<(), E>,
    ) -> core::result::Result<OpenedTransport<Buff>, TransportError>
    where
        E: Display,
    {
        todo!()
    }

    pub async fn async_connect<E>(
        mut self,
        read: impl AsyncFnMut(&mut [u8]) -> core::result::Result<usize, E>,
        write: impl AsyncFnMut(&[u8]) -> core::result::Result<(), E>,
    ) -> core::result::Result<OpenedTransport<Buff>, TransportError>
    where
        E: Display,
    {
        todo!()
    }
}

pub struct OpenedTransport<Buff> {
    tx: TransportTx<Buff>,
    rx: TransportRx<Buff>,
}
