use core::time::Duration;

use crate::fields::Resolution;

pub struct TransportTx<Buff> {
    pub(crate) buff: Buff,

    pub(crate) batch_size: usize,
    pub(crate) sn: u32,
    pub(crate) resolution: Resolution,
    pub(crate) lease: Duration,
}

impl<Buff> TransportTx<Buff> {
    pub(crate) fn new(
        buff: Buff,

        batch_size: usize,
        sn: u32,
        resolution: Resolution,
        lease: Duration,
    ) -> Self {
        Self {
            buff,
            batch_size,
            sn,
            resolution,
            lease,
        }
    }
}
