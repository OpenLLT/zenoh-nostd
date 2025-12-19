use crate::{fields::*, msgs::*, *};

#[derive(ZStruct, Debug, PartialEq, Default, Clone)]
#[zenoh(header = "Z|_|C|ID:5=0x4")]
pub struct Reply<'a> {
    #[zenoh(presence = header(C), default = ConsolidationMode::default())]
    pub consolidation: ConsolidationMode,

    pub payload: PushBody<'a>,
}
