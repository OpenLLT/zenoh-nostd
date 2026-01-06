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

    pub(crate) fn set_streamed(&mut self) {
        self.streamed = true;
    }

    pub fn decode(&mut self, read: &[u8]) -> core::result::Result<(), TransportError>
    where
        Buff: AsMut<[u8]> + AsRef<[u8]>,
    {
        if read.is_empty() || self.state == State::Closed {
            return Ok(());
        }

        let full_size = core::cmp::min(self.buff.as_ref().len(), self.batch_size);
        let left = full_size - self.cursor;
        let buff_mut = &mut self.buff.as_mut()[self.cursor..full_size];
        let len = if self.streamed {
            if 2 > left {
                crate::zbail!(@log TransportError::TransportTooSmall);
            }

            let mut len = [0u8; 2];
            len.copy_from_slice(&read[..2]);

            let len = u16::from_le_bytes(len) as usize;
            if len > left {
                crate::zbail!(@log TransportError::TransportIsFull)
            }
            if len > read.len() {
                crate::zbail!(@log TransportError::InvalidAttribute)
            }

            buff_mut[..len].copy_from_slice(&read[2..2 + len]);
            len
        } else {
            let len = read.len();
            if len > left {
                crate::zbail!(@log TransportError::TransportIsFull)
            }
            buff_mut[..len].copy_from_slice(&read[..len]);
            len
        };

        if len != 0 {
            self.state = State::Used;
        }

        self.cursor += len;

        Ok(())
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

        let mut read = |bytes: &mut [u8]| -> core::result::Result<usize, TransportError> {
            read(bytes).map_err(|e| {
                crate::error!("{e}");
                TransportError::CouldNotRead
            })
        };

        let full_size = core::cmp::min(self.buff.as_ref().len(), self.batch_size);
        let left = full_size - self.cursor;
        let buff_mut = &mut self.buff.as_mut()[self.cursor..full_size];

        let len = if self.streamed {
            if 2 > left {
                crate::zbail!(@log TransportError::TransportTooSmall);
            }

            let mut len = [0u8; 2];
            let l = read(&mut len)?;
            if l == 0 {
                return Ok(());
            } else if l != 2 {
                crate::zbail!(@log TransportError::InvalidAttribute)
            }

            let len = u16::from_le_bytes(len) as usize;
            if len > left {
                crate::zbail!(@log TransportError::TransportIsFull)
            }

            if read(&mut buff_mut[..len])? != len {
                crate::zbail!(@log TransportError::InvalidAttribute)
            }

            len
        } else {
            let len = read(&mut buff_mut[..])?;
            if len == 0 {
                return Ok(());
            }
            if len > left {
                crate::zbail!(@log TransportError::TransportIsFull)
            }
            len
        };

        if len != 0 {
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
