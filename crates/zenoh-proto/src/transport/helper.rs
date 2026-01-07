use crate::{TransportError, msgs::*};

pub fn write_streamed<'a>(
    buff: &'a mut [u8],
    msg: TransportMessage,
    streamed: bool,
) -> core::result::Result<&'a [u8], TransportError> {
    let slice = if streamed {
        if buff.len() < 2 {
            crate::zbail!(@log TransportError::TransportTooSmall);
        }

        &mut buff[2..]
    } else {
        &mut buff[..]
    };

    let len = crate::codec::transport_encoder(slice, core::iter::once(msg)).sum::<usize>();
    let len = if streamed {
        let length = (len as u16).to_le_bytes();
        buff[..2].copy_from_slice(&length);
        len + 2
    } else {
        len
    };

    Ok(&buff[..len])
}

pub fn read_streamed<'a>(
    buff: &'a mut [u8],
    mut with: impl FnMut(&mut [u8]) -> core::result::Result<usize, TransportError>,
    streamed: bool,
) -> core::result::Result<&'a [u8], TransportError> {
    let len = if streamed {
        if 2 > buff.len() {
            crate::zbail!(@log TransportError::TransportTooSmall);
        }

        let mut len = [0u8; 2];
        let l = with(&mut len)?;

        if l == 0 {
            return Ok(&[]);
        } else if l != 2 {
            crate::zbail!(@log TransportError::InvalidAttribute)
        }

        let len = u16::from_le_bytes(len) as usize;
        if len > u16::MAX as usize || len > buff.len() {
            crate::zbail!(@log TransportError::InvalidAttribute);
        }

        if with(&mut buff[..len])? != len {
            crate::zbail!(@log TransportError::InvalidAttribute)
        }

        len
    } else {
        let len = with(&mut buff[..])?;
        if len == 0 {
            return Ok(&[]);
        }

        if len > buff.len() {
            crate::zbail!(@log TransportError::InvalidAttribute)
        }

        len
    };

    Ok(&buff[..len])
}
