struct UninitTransport {}

struct InitTransport {
    tx: TransportTx,
    rx: TransportRx,
}

impl InitTransport {
    fn split(self) -> (TransportTx, TransportRx) {
        (self.tx, self.rx)
    }
}

struct TransportTx {}

impl TransportTx {
    fn push(&mut self, x: u32) {}
    fn batch(&mut self, x: impl Iterator<Item = u32>) {}
    fn flush(&mut self) -> &[u8] {
        &[]
    }
}

struct TransportRx {}

impl TransportRx {
    fn feed(&mut self, x: &[u8]) {}
    fn flush(&mut self) -> impl Iterator<Item = u32> {
        [].into_iter()
    }
}

fn main() {}
