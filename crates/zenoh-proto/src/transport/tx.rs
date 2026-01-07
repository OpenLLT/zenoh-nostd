use core::time::Duration;

use crate::{TransportError, ZInstant, fields::Resolution, msgs::NetworkMessage};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum State {
    Opened,
    Used,
    Synchronized { last_received: ZInstant },
    Closed,
}

#[derive(Debug)]
pub struct TransportTx<Buff> {
    buff: Buff,
    streamed: bool,
    cursor: usize,
    batch_size: usize,

    sn: u32,
    resolution: Resolution,
    lease: Duration,

    state: State,
}

impl<Buff> TransportTx<Buff> {
    pub(crate) fn new(
        buff: Buff,

        streamed: bool,
        batch_size: usize,
        sn: u32,
        resolution: Resolution,
        lease: Duration,
    ) -> Self {
        Self {
            buff,
            streamed,
            cursor: if streamed { 2 } else { 0 },
            batch_size,
            sn,
            resolution,
            lease,
            state: State::Opened,
        }
    }

    pub fn encode<'a>(
        &mut self,
        msgs: impl Iterator<Item = NetworkMessage<'a>>,
    ) -> core::result::Result<(), TransportError>
    where
        Buff: AsMut<[u8]> + AsRef<[u8]>,
    {
        let max = core::cmp::min(self.buff.as_ref().len(), self.batch_size);
        let buff = &mut self.buff.as_mut()[self.cursor..max];

        let len =
            crate::codec::network_encoder(buff, msgs, &mut self.sn, self.resolution).sum::<usize>();

        if len != 0 {
            self.state = State::Used;
        }

        self.cursor += len;

        Ok(())
    }

    pub fn encode_ref<'a>(
        &mut self,
        msgs: impl Iterator<Item = &'a NetworkMessage<'a>>,
    ) -> core::result::Result<(), TransportError>
    where
        Buff: AsMut<[u8]> + AsRef<[u8]>,
    {
        let max = core::cmp::min(self.buff.as_ref().len(), self.batch_size);
        let buff = &mut self.buff.as_mut()[self.cursor..max];

        let len = crate::codec::network_encoder_ref(buff, msgs, &mut self.sn, self.resolution)
            .sum::<usize>();

        if len != 0 {
            self.state = State::Used;
        }

        self.cursor += len;

        Ok(())
    }

    pub fn flush(&mut self) -> Option<&'_ [u8]>
    where
        Buff: AsMut<[u8]> + AsRef<[u8]>,
    {
        let size = core::cmp::min(
            self.buff.as_ref().len(),
            core::cmp::min(self.batch_size, self.cursor),
        );

        if self.streamed {
            if size < 2 {
                crate::zbail!(@None TransportError::TransportTooSmall);
            }

            let len = ((size - 2) as u16).to_le_bytes();
            self.buff.as_mut()[..2].copy_from_slice(&len);
            self.cursor = 2;
        } else {
            self.cursor = 0;
        }

        let buff_ref = &self.buff.as_ref()[..size];
        if size > 0 { Some(buff_ref) } else { None }
    }

    pub fn sync(&mut self, now: ZInstant) {
        if let State::Synchronized { .. } = self.state {
            if now.0 > self.next_timeout().0 {
                self.state = State::Closed;
            }
        }

        if self.state == State::Used {
            self.state = State::Synchronized { last_received: now };
        };
    }

    pub fn next_timeout(&self) -> ZInstant {
        match self.state {
            State::Opened | State::Closed | State::Used => Duration::from_secs(0).into(),
            State::Synchronized { last_received } => (last_received.0 + self.lease / 4).into(),
        }
    }
}
