// use crate::{OpenedTransport, TransportError, establishment::State};

use crate::establishment::State;

#[derive(Debug)]
pub struct Handshake<Buff> {
    tx: Buff,
    rx: Buff,

    streamed: bool,
    state: State,
}

impl<Buff> Handshake<Buff> {
    pub(crate) fn new(tx: Buff, rx: Buff, streamed: bool, state: State) -> Self {
        Self {
            tx,
            rx,
            streamed,
            state,
        }
    }

    pub fn init<E>(
        &mut self,
        write: impl FnMut(&[u8]) -> core::result::Result<(), E>,
    ) -> core::result::Result<(), E> {
    }
}

// pub(crate) struct Handshake<Buff, Read, Write> {
//     recv: Buff,
//     send: Buff,

//     streamed: bool,
//     read: Read,
//     write: Write,

//     state: State,
// }

// impl<Buff, Read, Write> Handshake<Buff, Read, Write> {
//     pub fn poll<E>(&mut self) -> Option<OpenedTransport<Buff>>
//     where
//         Buff: AsMut<[u8]> + AsRef<[u8]>,
//         Read: FnMut(&mut [u8]) -> core::result::Result<usize, E>,
//         Write: FnMut(&[u8]) -> core::result::Result<(), E>,
//         E: core::error::Error,
//     {
//         let mut read = |bytes: &mut [u8]| -> core::result::Result<usize, TransportError> {
//             (self.read)(bytes).map_err(|e| {
//                 crate::error!("{e}");
//                 TransportError::CouldNotRead
//             })
//         };

//         let mut write = |bytes: &[u8]| -> core::result::Result<(), TransportError> {
//             (self.write)(bytes).map_err(|e| {
//                 crate::error!("{e}");
//                 TransportError::CouldNotWrite
//             })
//         };

//         if !self.streamed {
//             let buff_mut = self.recv.as_mut();
//             let mut len = read(buff_mut).ok()?;

//             let ret = self
//                 .state
//                 .poll(&buff_mut[..len], (self.send.as_mut(), &mut len));

//             if len > 0 {
//                 write(&self.send.as_ref()[..len]).ok()?;
//             }

//             ret.map(|description| (description, self.streamed, self.recv, self.send).into())
//         } else {
//             let buff_mut = self.recv.as_mut();

//             if buff_mut.len() < 2 {
//                 crate::zbail!(@None TransportError::TransportTooSmall);
//             }
//             let mut len = [0u8; 2];
//             let l = read(&mut len).ok()?;
//             if l == 0 {
//                 return None;
//             } else if l != 2 {
//                 crate::zbail!(@None TransportError::InvalidAttribute);
//             }

//             let mut len = u16::from_le_bytes(len) as usize;
//             if len > buff_mut.len() {
//                 crate::zbail!(@None TransportError::TransportIsFull)
//             }

//             if read(&mut buff_mut[..len]).ok()? != len {
//                 crate::zbail!(@None TransportError::InvalidAttribute)
//             }

//             if self.send.as_ref().len() < 2 {
//                 crate::zbail!(@None TransportError::TransportTooSmall)
//             }

//             let ret = self
//                 .state
//                 .poll(&buff_mut[..len], (&mut self.send.as_mut()[2..], &mut len));

//             if len > 0 {
//                 if self.send.as_ref().len() < 2 + len {
//                     crate::zbail!(@None TransportError::TransportTooSmall)
//                 }

//                 self.send.as_mut()[..2].copy_from_slice(&u16::to_le_bytes(len as u16));
//                 write(&self.send.as_ref()[..2 + len]).ok()?;
//             }

//             ret.map(|description| (description, self.streamed, self.recv, self.send).into())
//         }
//     }

//     // pub fn connect(self) -> Description {}
//     // pub fn listen(self) -> Description {}
// }
