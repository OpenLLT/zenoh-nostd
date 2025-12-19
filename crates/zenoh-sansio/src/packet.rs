use crate::Id;

#[derive(Debug, PartialEq)]
pub struct Packet<'a>(&'a [u8]);

impl<'a> Packet<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self(data)
    }
}

impl<'a> AsRef<[u8]> for Packet<'a> {
    fn as_ref(&self) -> &[u8] {
        self.0
    }
}

#[derive(Debug, PartialEq)]
pub struct PacketWithDst<'a>(Id, Packet<'a>);

impl<'a> PacketWithDst<'a> {
    pub fn new(dst: Id, data: &'a [u8]) -> Self {
        Self(dst, Packet::new(data))
    }

    pub fn dst(&self) -> Id {
        self.0
    }
}

impl<'a> AsRef<[u8]> for PacketWithDst<'a> {
    fn as_ref(&self) -> &[u8] {
        self.1.as_ref()
    }
}
