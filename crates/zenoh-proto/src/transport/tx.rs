use crate::{
    TransportError, exts::QoS, fields::Reliability, msgs::NetworkMessage,
    transport::state::TransportState,
};

#[derive(Debug)]
pub struct TransportTx<Buff> {
    pub(crate) streamed: bool,
    pub(crate) tx: Buff,
    pub(crate) cursor: usize,

    pub(crate) batch_size: u16,
    pub(crate) next_sn: u32,

    pub(crate) last_qos: Option<QoS>,
    pub(crate) last_reliability: Option<Reliability>,
}

impl<Buff> TransportTx<Buff> {
    pub fn new(tx: Buff) -> Self
    where
        Buff: AsRef<[u8]>,
    {
        Self {
            batch_size: tx.as_ref().len() as u16,
            tx,
            cursor: 0,
            streamed: false,
            next_sn: 0,
            last_qos: None,
            last_reliability: None,
        }
    }

    pub(crate) fn sync(&mut self, state: &TransportState) -> &mut Self {
        if self.next_sn == 0 {
            self.next_sn = *state.sn();
        }

        self
    }

    fn push_internal(
        streamed: bool,
        batch_size: usize,
        cursor: &mut usize,
        buffer: &mut [u8],
        last_reliability: &mut Option<Reliability>,
        last_qos: &mut Option<QoS>,
        next_sn: &mut u32,
        msg: &NetworkMessage,
    ) -> core::result::Result<(), TransportError> {
        let batch_size = core::cmp::min(batch_size as usize, buffer.len());
        let mut buffer = &mut buffer[*cursor..batch_size];

        if *cursor == 0 {
            if streamed {
                if buffer.len() < 2 {
                    crate::zbail!(@log TransportError::TransportIsFull);
                }

                buffer = &mut buffer[2..];
                *cursor += 2;
            }
        }

        let start = buffer.len();
        if msg
            .z_encode(&mut buffer, last_reliability, last_qos, next_sn)
            .is_err()
        {
            crate::zbail!(@log TransportError::MessageTooLargeForBatch)
        }

        *cursor += start - buffer.len();
        Ok(())
    }

    pub fn push(&mut self, msg: &NetworkMessage) -> core::result::Result<(), TransportError>
    where
        Buff: AsMut<[u8]>,
    {
        let batch_size = core::cmp::min(self.batch_size as usize, self.tx.as_mut().len());
        let mut buffer = &mut self.tx.as_mut()[self.cursor..batch_size];

        if self.cursor == 0 {
            if self.streamed {
                if buffer.len() < 2 {
                    crate::zbail!(@log TransportError::TransportIsFull);
                }

                buffer = &mut buffer[2..];
                self.cursor += 2;
            }
        }

        let reliability = &mut self.last_reliability;
        let qos = &mut self.last_qos;
        let sn = &mut self.next_sn;

        let start = buffer.len();
        if msg.z_encode(&mut buffer, reliability, qos, sn).is_err() {
            crate::zbail!(@log TransportError::MessageTooLargeForBatch)
        }

        self.cursor += start - buffer.len();
        Ok(())
    }

    pub fn flush(&mut self) -> Option<&'_ [u8]>
    where
        Buff: AsMut<[u8]>,
    {
        let buffer = &mut self.tx.as_mut()[..self.cursor];

        if self.streamed {
            if self.cursor <= 2 {
                return None;
            }

            let length = (self.cursor - 2) as u16;
            let length = length.to_le_bytes();
            buffer[..2].copy_from_slice(&length);
        }

        if self.cursor == 0 {
            return None;
        }

        self.cursor = 0;
        Some(buffer)
    }

    pub fn batch<'a, 'b>(
        &'a mut self,
        msgs: impl Iterator<Item = NetworkMessage<'b>>,
    ) -> impl Iterator<Item = &'a [u8]>
    where
        Buff: AsMut<[u8]>,
    {
        let mut msgs = msgs.peekable();

        core::iter::from_fn(move || {
            let _ = 3;

            while let Some(msg) = msgs.peek() {}

            self.flush()
        })

        // let streamed = self.streamed;
        // let mut buffer = self.tx.as_mut();
        // let batch_size = core::cmp::min(self.batch_size as usize, buffer.len());

        // let mut msgs = msgs.peekable();

        // let reliability = &mut self.last_reliability;
        // let qos = &mut self.last_qos;
        // let sn = &mut self.next_sn;

        // core::iter::from_fn(move || {
        //     let batch_size = core::cmp::min(batch_size as usize, buffer.len());
        //     let batch = &mut buffer[..batch_size];

        //     if streamed && batch_size < 2 {
        //         return None;
        //     }

        //     let mut writer = &mut batch[if streamed { 2 } else { 0 }..];
        //     let start = writer.len();

        //     let mut length = 0;
        //     while let Some(msg) = msgs.peek() {
        //         if msg.z_encode(&mut writer, reliability, qos, sn).is_ok() {
        //             length = start - writer.len();
        //             msgs.next();
        //         } else {
        //             break;
        //         }
        //     }

        //     if length == 0 {
        //         return None;
        //     }

        //     if streamed {
        //         let l = (length as u16).to_be_bytes();
        //         batch[..2].copy_from_slice(&l);
        //     }

        //     let (ret, remain) =
        //         core::mem::take(&mut buffer).split_at_mut(length + if streamed { 2 } else { 0 });
        //     buffer = remain;

        //     Some(&ret[..])
        // })
    }
}
