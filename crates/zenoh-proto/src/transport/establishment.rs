use core::time::Duration;

use sha3::{
    Shake128,
    digest::{ExtendableOutput, Update, XofReader},
};

use crate::{TransportError, fields::*, msgs::*};

/// Everything that describes an Opened Transport between two peers
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) struct Description {
    pub mine_zid: ZenohIdProto,

    pub batch_size: u16,
    pub resolution: Resolution,

    pub mine_lease: Duration,
    pub other_lease: Duration,

    pub sn: u32,
    pub other_sn: u32,

    pub other_zid: ZenohIdProto,
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum State {
    WaitingInitSyn {
        /// Mine zid
        mine_zid: ZenohIdProto,
        /// Mine startup batch_size
        mine_batch_size: u16,
        /// Mine startup resolution
        mine_resolution: Resolution,
        /// Mine lease,
        mine_lease: Duration,
    },
    WaitingOpenSyn {
        /// Mine zid
        mine_zid: ZenohIdProto,
        /// Mine startup batch_size
        mine_batch_size: u16,
        /// Mine startup resolution
        mine_resolution: Resolution,
        /// Mine lease,
        mine_lease: Duration,
    },
    WaitingInitAck {
        /// Mine zid
        mine_zid: ZenohIdProto,
        /// Mine startup batch_size
        mine_batch_size: u16,
        /// Mine startup resolution
        mine_resolution: Resolution,
        /// Mine lease,
        mine_lease: Duration,
    },
    WaitingOpenAck {
        /// Mine zid
        mine_zid: ZenohIdProto,
        /// Negotiated batch_size
        batch_size: u16,
        /// Negotiated resolution
        resolution: Resolution,
        /// Computed sn,
        sn: u32,
        /// Mine lease,
        mine_lease: Duration,
        /// Peer zid
        other_zid: ZenohIdProto,
    },
    Opened(Description),
}

impl State {
    pub(crate) fn poll(
        &mut self,
        read: &[u8],
        write: (&mut [u8], &mut usize),
    ) -> Option<Description> {
        *write.1 = 0;

        if let Self::Opened(description) = &self {
            return Some(*description);
        }

        for (msg, buff) in crate::codec::transport_decoder(read) {
            let response = match msg {
                // Don't do any computation at this stage. Pass the relevant values as the cookie
                // for future computation.
                TransportMessage::InitSyn(_) => match *self {
                    Self::WaitingInitSyn {
                        mine_zid,
                        mine_batch_size,
                        mine_resolution,
                        mine_lease,
                    } => {
                        *self = Self::WaitingOpenSyn {
                            mine_zid: mine_zid,
                            mine_batch_size: mine_batch_size,
                            mine_resolution: mine_resolution,
                            mine_lease: mine_lease,
                        };

                        TransportMessage::InitAck(InitAck {
                            identifier: InitIdentifier {
                                zid: mine_zid,
                                ..Default::default()
                            },
                            resolution: InitResolution {
                                resolution: mine_resolution,
                                batch_size: BatchSize(mine_batch_size),
                            },
                            cookie: buff, // TODO: cypher
                            ..Default::default()
                        })
                    }
                    _ => crate::zbail!(@continue TransportError::StateCantHandle),
                },
                // Negotiate values, pass the cookie back
                TransportMessage::InitAck(ack) => match *self {
                    Self::WaitingInitAck {
                        mine_zid,
                        mine_batch_size,
                        mine_resolution,
                        mine_lease,
                    } => {
                        crate::debug!(
                            "Received InitAck on transport {:?} -> NEW!({:?})",
                            mine_zid,
                            ack.identifier.zid
                        );

                        let batch_size = mine_batch_size.min(ack.resolution.batch_size.0);
                        let resolution = {
                            let mut res = Resolution::default();
                            let i_fsn_res = ack.resolution.resolution.get(Field::FrameSN);
                            let m_fsn_res = mine_resolution.get(Field::FrameSN);
                            if i_fsn_res > m_fsn_res {
                                crate::zbail!(@continue TransportError::InvalidAttribute);
                            }
                            res.set(Field::FrameSN, i_fsn_res);
                            let i_rid_res = ack.resolution.resolution.get(Field::RequestID);
                            let m_rid_res = mine_resolution.get(Field::RequestID);
                            if i_rid_res > m_rid_res {
                                crate::zbail!(@continue TransportError::InvalidAttribute);
                            }
                            res.set(Field::RequestID, i_rid_res);
                            res
                        };
                        let sn = {
                            let mut hasher = Shake128::default();
                            hasher.update(&mine_zid.as_le_bytes()[..mine_zid.size()]);
                            hasher.update(
                                &ack.identifier.zid.as_le_bytes()[..ack.identifier.zid.size()],
                            );
                            let mut array = (0 as u32).to_le_bytes();
                            hasher.finalize_xof().read(&mut array);
                            u32::from_le_bytes(array)
                                & match mine_resolution.get(Field::FrameSN) {
                                    Bits::U8 => u8::MAX as u32 >> 1,
                                    Bits::U16 => u16::MAX as u32 >> 2,
                                    Bits::U32 => u32::MAX as u32 >> 4,
                                    Bits::U64 => u64::MAX as u32 >> 1,
                                }
                        };

                        *self = Self::WaitingOpenAck {
                            mine_zid,
                            batch_size,
                            resolution,
                            sn,
                            mine_lease: mine_lease,
                            other_zid: ack.identifier.zid,
                        };

                        TransportMessage::OpenSyn(OpenSyn {
                            lease: mine_lease,
                            sn,
                            cookie: ack.cookie,
                            ..Default::default()
                        })
                    }
                    _ => crate::zbail!(@continue TransportError::StateCantHandle),
                },
                // Negotiate values, open the transport. Ack the open
                TransportMessage::OpenSyn(open) => match *self {
                    Self::WaitingOpenSyn {
                        mine_zid,
                        mine_batch_size,
                        mine_resolution,
                        mine_lease,
                    } => {
                        // TODO: decypher cookie
                        let syn =
                            match crate::codec::transport_decoder(open.cookie).find_map(|msg| {
                                match msg.0 {
                                    TransportMessage::InitSyn(syn) => Some(syn),
                                    _ => None,
                                }
                            }) {
                                Some(syn) => syn,
                                None => crate::zbail!(@continue TransportError::InvalidAttribute),
                            };

                        crate::debug!(
                            "Received OpenSyn on transport {:?} -> NEW!({:?})",
                            mine_zid,
                            syn.identifier.zid
                        );

                        let batch_size = mine_batch_size.min(syn.resolution.batch_size.0);
                        let resolution = {
                            let mut res = Resolution::default();
                            let i_fsn_res = syn.resolution.resolution.get(Field::FrameSN);
                            let m_fsn_res = mine_resolution.get(Field::FrameSN);
                            if i_fsn_res > m_fsn_res {
                                crate::zbail!(@continue TransportError::InvalidAttribute);
                            }
                            res.set(Field::FrameSN, i_fsn_res);
                            let i_rid_res = syn.resolution.resolution.get(Field::RequestID);
                            let m_rid_res = mine_resolution.get(Field::RequestID);
                            if i_rid_res > m_rid_res {
                                crate::zbail!(@continue TransportError::InvalidAttribute);
                            }
                            res.set(Field::RequestID, i_rid_res);
                            res
                        };
                        let sn = {
                            let mut hasher = Shake128::default();
                            hasher.update(&mine_zid.as_le_bytes()[..mine_zid.size()]);
                            hasher.update(
                                &syn.identifier.zid.as_le_bytes()[..syn.identifier.zid.size()],
                            );
                            let mut array = (0 as u32).to_le_bytes();
                            hasher.finalize_xof().read(&mut array);
                            u32::from_le_bytes(array)
                                & match mine_resolution.get(Field::FrameSN) {
                                    Bits::U8 => u8::MAX as u32 >> 1,
                                    Bits::U16 => u16::MAX as u32 >> 2,
                                    Bits::U32 => u32::MAX as u32 >> 4,
                                    Bits::U64 => u64::MAX as u32 >> 1,
                                }
                        };

                        *self = Self::Opened(Description {
                            mine_zid,
                            batch_size,
                            resolution,
                            mine_lease,
                            other_lease: open.lease,
                            sn,
                            other_sn: open.sn,
                            other_zid: syn.identifier.zid,
                        });

                        TransportMessage::OpenAck(OpenAck {
                            lease: mine_lease,
                            sn: sn,
                            ..Default::default()
                        })
                    }
                    _ => crate::zbail!(@continue TransportError::StateCantHandle),
                },
                // Open the transport
                TransportMessage::OpenAck(ack) => match *self {
                    Self::WaitingOpenAck {
                        mine_zid,
                        batch_size,
                        resolution,
                        sn,
                        mine_lease,
                        other_zid,
                    } => {
                        *self = Self::Opened(Description {
                            mine_zid,
                            batch_size,
                            resolution,
                            mine_lease,
                            other_lease: ack.lease,
                            sn,
                            other_sn: ack.sn,
                            other_zid,
                        });

                        continue;
                    }
                    _ => crate::zbail!(@continue TransportError::StateCantHandle),
                },
                _ => continue,
            };

            *write.1 += crate::codec::transport_encoder(
                &mut write.0[(*write.1)..],
                core::iter::once(response),
            )
            .sum::<usize>();
        }

        match self {
            Self::Opened(description) => Some(*description),
            _ => None,
        }
    }

    pub(crate) fn opened(&self) -> bool {
        matches!(self, Self::Opened { .. })
    }
}

#[test]
fn transport_state_handshake_regular() {
    let mut a = State::WaitingInitSyn {
        mine_zid: ZenohIdProto::default(),
        mine_batch_size: 512,
        mine_resolution: Resolution::default(),
        mine_lease: Duration::from_secs(30),
    };

    let b_zid = ZenohIdProto::default();
    let mut b = State::WaitingInitAck {
        mine_zid: b_zid,
        mine_batch_size: 1025,
        mine_resolution: Resolution::default(),
        mine_lease: Duration::from_secs(37),
    };

    let mut buff1 = [0u8; 512];
    let mut buff2 = [0u8; 512];

    // Simulate 'b' sending InitSyn
    let mut len: usize = crate::codec::transport_encoder(
        &mut buff1,
        core::iter::once(TransportMessage::InitSyn(InitSyn {
            identifier: InitIdentifier {
                zid: b_zid,
                ..Default::default()
            },
            resolution: InitResolution {
                resolution: Resolution::default(),
                batch_size: BatchSize(1025),
            },
            ..Default::default()
        })),
    )
    .sum();

    // a receives InitSyn and goes to WaitingOpenSyn state and writes an InitAck on buff2
    a.poll(&buff1[..len], (&mut buff2, &mut len));
    // b receives an InitAck and goes to WaitingOpenAck and writes an OpenSyn on buff1
    b.poll(&buff2[..len], (&mut buff1, &mut len));
    // a receives an OpenSyn and goes to Opened and writes an OpenAck on buff2
    a.poll(&buff1[..len], (&mut buff2, &mut len));
    // b receives an OpenAck and goes to Opened
    b.poll(&buff2[..len], (&mut buff1, &mut len));

    assert!(a.opened() && b.opened());
}

#[test]
fn transport_state_handshake_skip() {
    let mut a = State::WaitingInitSyn {
        mine_zid: ZenohIdProto::default(),
        mine_batch_size: 512,
        mine_resolution: Resolution::default(),
        mine_lease: Duration::from_secs(30),
    };

    let mut socket = [0u8; 512];
    let writer = &mut socket[..];

    let mut len = crate::codec::transport_encoder(
        writer,
        core::iter::once(TransportMessage::InitSyn(InitSyn::default())),
    )
    .sum::<usize>();

    let (cookie, remain) = writer.split_at_mut(len);
    len += crate::codec::transport_encoder(
        remain,
        core::iter::once(TransportMessage::OpenSyn(OpenSyn {
            cookie,
            ..Default::default()
        })),
    )
    .sum::<usize>();

    a.poll(&socket[..len], (&mut [0u8; 512], &mut 0));
    assert!(a.opened())
}
