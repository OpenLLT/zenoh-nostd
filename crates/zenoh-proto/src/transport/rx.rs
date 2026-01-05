use core::time::Duration;

use crate::fields::Resolution;

pub struct TransportRx<Buff> {
    buff: Buff,

    batch_size: usize,
    sn: u32,
    resolution: Resolution,
    lease: Duration,
}

impl<Buff> TransportRx<Buff> {
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
