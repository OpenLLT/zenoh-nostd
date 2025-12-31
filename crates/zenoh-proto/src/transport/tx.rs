use crate::{
    ZEncode,
    exts::QoS,
    fields::Reliability,
    msgs::NetworkMessage,
    transport::{scope::TransportStateScoped, state::TransportState},
};

#[derive(Debug)]
pub struct TransportTx<Buff> {
    pub(crate) streamed: bool,
    tx: Buff,

    batch_size: u16,
    next_sn: u32,

    last_qos: Option<QoS>,
    last_reliability: Option<Reliability>,
}

impl<Buff> TransportTx<Buff> {
    pub fn new(tx: Buff) -> Self
    where
        Buff: AsRef<[u8]>,
    {
        Self {
            batch_size: tx.as_ref().len() as u16,
            tx,
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

    pub fn write<'a, 'b>(
        &'a mut self,
        msgs: impl Iterator<Item = NetworkMessage<'b>>,
    ) -> impl Iterator<Item = &'a [u8]>
    where
        Buff: AsMut<[u8]>,
    {
        let streamed = self.streamed;
        let mut buffer = self.tx.as_mut();
        let batch_size = core::cmp::min(self.batch_size as usize, buffer.len());

        let mut msgs = msgs.peekable();

        let reliability = &mut self.last_reliability;
        let qos = &mut self.last_qos;
        let sn = &mut self.next_sn;

        core::iter::from_fn(move || {
            let batch_size = core::cmp::min(batch_size as usize, buffer.len());
            let batch = &mut buffer[..batch_size];

            if streamed && batch_size < 2 {
                return None;
            }

            let mut writer = &mut batch[if streamed { 2 } else { 0 }..];
            let start = writer.len();

            let mut length = 0;
            while let Some(msg) = msgs.peek() {
                if msg.z_encode(&mut writer, reliability, qos, sn).is_ok() {
                    length = start - writer.len();
                    msgs.next();
                } else {
                    break;
                }
            }

            if length == 0 {
                return None;
            }

            if streamed {
                let l = (length as u16).to_be_bytes();
                batch[..2].copy_from_slice(&l);
            }

            let (ret, remain) =
                core::mem::take(&mut buffer).split_at_mut(length + if streamed { 2 } else { 0 });
            buffer = remain;

            Some(&ret[..])
        })
    }

    pub fn interact<'a>(&mut self, state: &mut TransportStateScoped<'a>) -> Option<&'_ [u8]>
    where
        Buff: AsMut<[u8]>,
    {
        if let Some(pending) = state.pending.take() {
            let batch_size = core::cmp::min(self.batch_size as usize, self.tx.as_mut().len());
            let batch = &mut self.tx.as_mut()[..batch_size];

            if self.streamed && batch_size < 2 {
                return None;
            }

            let mut writer = &mut batch[if self.streamed { 2 } else { 0 }..];
            let start = writer.len();

            let length = if pending.0.z_encode(&mut writer).is_ok() {
                start - writer.len()
            } else {
                crate::error!("Couldn't encode msg {:?}", pending.0);
                return None;
            };

            if length == 0 {
                return None;
            }

            if self.streamed {
                let l = (length as u16).to_be_bytes();
                batch[..2].copy_from_slice(&l);
            }

            let (ret, _) = self
                .tx
                .as_mut()
                .split_at(length + if self.streamed { 2 } else { 0 });

            Some(&ret[..])
        } else {
            None
        }
    }
}
