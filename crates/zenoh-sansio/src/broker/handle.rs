use core::time::Duration;

use zenoh_proto::{
    AdvancingWriter,
    fields::{Resolution, ZenohIdProto},
    msgs::OpenSyn,
};

use crate::{Id, PacketWithDst, broker::State};

pub(super) fn init_ack<'a, const N: usize, const P: usize>(
    zid: &ZenohIdProto,
    lease: Duration,
    north: &mut State,
    ack: zenoh_proto::msgs::InitAck<'_>,
    id: Id,
    w: &mut AdvancingWriter<'a>,
    packets: &mut heapless::Vec<PacketWithDst<'a>, P>,
) -> crate::ZResult<()> {
    if id != Id::North {
        zenoh_proto::warn!(
            "Received InitAck from South id {:?} - {}",
            id,
            zenoh_proto::zctx!()
        );
        return Ok(());
    }

    if let State::NotInitialized = north {
        let resolution = crate::establish::negotiate_resolution(
            &Resolution::default(),
            &ack.resolution.resolution,
        )?;
        let sn = crate::establish::negotiate_sn(zid, &ack.identifier.zid, &resolution);
        let batch_size =
            crate::establish::negotiate_batch_size(N as u16, ack.resolution.batch_size.0);

        *north = State::Initialized {
            zid: ack.identifier.zid,
            batch_size,
            resolution,
            sn,
        };

        zenoh_proto::info!("North initialized: {:?} - {}", north, zenoh_proto::zctx!());

        let payload = w.unframed(&OpenSyn {
            cookie: ack.cookie,
            lease,
            sn,
            ..Default::default()
        })?;

        packets
            .push(PacketWithDst::new(Id::North, payload))
            .unwrap();
    } else {
        zenoh_proto::warn!(
            "Received duplicate InitAck from North - {}",
            zenoh_proto::zctx!()
        );
    }

    Ok(())
}

pub(super) fn open_ack<'a>(
    north: &mut State,
    ack: zenoh_proto::msgs::OpenAck<'_>,
    id: Id,
) -> crate::ZResult<()> {
    if id != Id::North {
        zenoh_proto::warn!(
            "Received OpenAck from South id {:?} - {}",
            id,
            zenoh_proto::zctx!()
        );
        return Ok(());
    }

    *north = match core::mem::replace(north, State::NotInitialized) {
        State::Initialized {
            zid,
            batch_size,
            resolution,
            sn,
        } => {
            zenoh_proto::info!(
                "North opened: lease={}ms - {}",
                ack.lease.as_millis(),
                zenoh_proto::zctx!()
            );
            State::Opened {
                zid,
                batch_size,
                resolution,
                sn,
                lease: ack.lease,
            }
        }
        other => {
            zenoh_proto::warn!(
                "Received OpenAck while not in Initialized state: {:?} - {}",
                other,
                zenoh_proto::zctx!()
            );
            other
        }
    };

    Ok(())
}
