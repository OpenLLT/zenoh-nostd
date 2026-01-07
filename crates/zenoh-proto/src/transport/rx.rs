use core::fmt::Display;
use core::time::Duration;

use crate::{
    TransportError, ZInstant,
    fields::Resolution,
    msgs::{Message, NetworkMessage, TransportMessage},
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum State {
    Opened,
    Used,
    Synchronized { last_received: ZInstant },
    Closed,
}

#[derive(Debug)]
pub struct TransportRx<Buff> {
    buff: Buff,

    cursor: usize,
    batch_size: usize,
    streamed: bool,

    sn: u32,
    resolution: Resolution,
    lease: Duration,

    state: State,
}

impl<Buff> TransportRx<Buff> {
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

            cursor: 0,
            batch_size,
            streamed,

            sn,
            resolution,
            lease,

            state: State::Opened,
        }
    }

    pub fn decode(&mut self, mut read: &[u8]) -> core::result::Result<(), TransportError>
    where
        Buff: AsMut<[u8]> + AsRef<[u8]>,
    {
        if read.is_empty() || self.state == State::Closed {
            return Ok(());
        }

        self.decode_with(|data| {
            let size = data.len().min(read.len());
            let (ret, remain) = read.split_at(size);
            data[..size].copy_from_slice(ret);
            read = remain;
            Ok::<_, TransportError>(size)
        })
    }

    pub fn decode_with<E>(
        &mut self,
        mut read: impl FnMut(&mut [u8]) -> core::result::Result<usize, E>,
    ) -> core::result::Result<(), TransportError>
    where
        Buff: AsMut<[u8]> + AsRef<[u8]>,
        E: Display,
    {
        if self.state == State::Closed {
            return Ok(());
        }

        let max = core::cmp::min(self.buff.as_ref().len(), self.batch_size);
        let buff = &mut self.buff.as_mut()[self.cursor..max];

        let len = super::helper::read_streamed(
            buff,
            |bytes: &mut [u8]| -> core::result::Result<usize, TransportError> {
                read(bytes).map_err(|e| {
                    crate::error!("{e}");
                    TransportError::CouldNotRead
                })
            },
            self.streamed,
        )?
        .len();

        if len > 0 {
            self.state = State::Used;
        }

        self.cursor += len;

        Ok(())
    }

    pub fn flush(&mut self) -> impl Iterator<Item = NetworkMessage<'_>>
    where
        Buff: AsMut<[u8]> + AsRef<[u8]>,
    {
        let size = core::cmp::min(
            self.buff.as_ref().len(),
            core::cmp::min(self.batch_size, self.cursor),
        );
        let buff_ref = &self.buff.as_ref()[..size];
        self.cursor = 0;

        crate::codec::decoder(buff_ref, &mut self.sn, self.resolution)
            .map(|msg| msg.0)
            .filter_map(|msg| match msg {
                Message::Network(msg) => Some(msg),
                Message::Transport(msg) => {
                    if let TransportMessage::Close(_) = msg {
                        self.state = State::Closed;
                    }

                    None
                }
            })
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
