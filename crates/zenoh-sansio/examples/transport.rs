// use std::{
//     io::{Read, Write},
//     net::TcpStream,
// };

// use zenoh_proto::{ZDecode, ZEncode, msgs::InitSyn};

// enum NetworkMessage<'a> {
//     A(InitSyn<'a>),
//     B(&'a [u8]),
// }

// enum TransportState {
//     Uninitialized,
//     Initialized,
//     Opened,
// }

// struct NetworkMessageIterator<'s> {
//     transport: &'s mut TransportState,
//     reader: &'s [u8],
// }

// impl<'t> Iterator for NetworkMessageIterator<'t> {
//     type Item = NetworkMessage<'t>;

//     fn next(&mut self) -> Option<Self::Item> {
//         let _ = 3;
//         let init_syn = InitSyn::z_decode(&mut self.reader).unwrap();
//         Some(NetworkMessage::A(init_syn))
//     }
// }

// struct MyTransport {
//     state: TransportState,
//     streamed: bool,

//     tx: [u8; 2048],
//     rx: [u8; 2048],
// }

// impl MyTransport {
//     fn state(&self) -> &TransportState {
//         &self.state
//     }

//     fn state_mut(&mut self) -> &mut TransportState {
//         &mut self.state
//     }

//     fn is_streamed(&self) -> bool {
//         self.streamed
//     }

//     fn send_with<'s, 'm>(
//         &'s mut self,
//         mut msgs: impl Iterator<Item = NetworkMessage<'m>>,
//     ) -> impl Iterator<Item = &'s [u8]> {
//         let mut w = &mut self.tx[..];
//         core::iter::from_fn(move || {
//             let msg = msgs.next()?;
//             match msg {
//                 NetworkMessage::A(i) => {
//                     let s = w.len();
//                     i.z_encode(&mut w).unwrap();
//                     let l = s - w.len();
//                     let (ret, remain) = core::mem::take(&mut w).split_at_mut(l);
//                     w = remain;
//                     Some(&ret[..])
//                 }
//                 _ => None,
//             }
//         })
//     }

//     fn recv_with<'s>(&'s mut self, mut f: impl FnMut(&mut [u8])) -> NetworkMessageIterator<'s> {
//         let mut l = [0u8; 2];
//         f(&mut l);
//         let l = u16::from_le_bytes(l) as usize;
//         f(&mut self.rx[..l]);
//         // core::iter::from_fn(f)
//         // let mut reader: &'s [u8] = &self.rx[..l];
//         // let mut state = self.state_mut();
//         let Self {
//             state,
//             streamed: _,
//             tx: _,
//             rx,
//         } = self;
//         // core::iter::from_fn(move || {
//         //     let _ = 3;
//         //     let init_syn = InitSyn::z_decode(&mut reader).unwrap();
//         //     Some(NetworkMessage::A(init_syn))
//         // })
//         NetworkMessageIterator {
//             transport: state,
//             reader: &rx[..l],
//         }
//     }
// }

use std::{
    io::{Read, Write},
    net::TcpStream,
};

use zenoh_proto::{
    Transport,
    msgs::{MessageIter, NetworkMessage},
};

fn session_process<'m>(
    _: impl Iterator<Item = NetworkMessage<'m>>,
) -> impl Iterator<Item = NetworkMessage<'m>> {
    core::iter::empty()
}

fn main() {
    let mut tcp = TcpStream::connect("aoza").unwrap();
    let mut transport = Transport::new(false, [0; 2048], [0; 2048], 2048, 0);

    loop {
        let (mut rx, tx) = transport.update(|data| {
            tcp.read_exact(data).unwrap();
        });

        while let Some(batch) = rx.next() {
            match batch {
                MessageIter::Transport(msgs) => {
                    for chunk in tx.send(msgs, core::iter::empty()) {
                        tcp.write_all(chunk).unwrap();
                    }
                }
                MessageIter::Network(msgs) => {
                    let ret = session_process(msgs);

                    for chunk in tx.send(core::iter::empty(), ret) {
                        tcp.write_all(chunk).unwrap();
                    }
                }
            }
        }
    }
}
