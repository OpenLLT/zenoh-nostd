use std::{io::Write, net::TcpStream};

use zenoh_proto::{
    ZEncode, ZFramed, ZUnframed,
    msgs::{InitSyn, Push},
    zerror::TransportError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum TransportMode {
    #[default]
    StartNewBatch,
    Blocking,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Performed {
    #[default]
    Nothing,
    StartedNewBatch(usize),
    Blocked(usize),
}

trait ZTransport {
    fn set_mode(&mut self, mode: TransportMode);
    fn batch_size(&self) -> usize;
    fn max_batches(&self) -> usize;

    fn unframed(&mut self, x: &impl ZEncode) -> core::result::Result<Performed, TransportError>;
    fn flush(&mut self, f: impl FnMut(&[u8]));
}

fn fill_whole_buff(mut x: impl ZTransport) {
    let mut tcp = TcpStream::connect("127.0.0.1:7447").unwrap();

    while x.unframed(&InitSyn::default()).is_ok() {}

    x.flush(|data| {
        tcp.write_all(data).unwrap();
    });
}

fn fill_partial_buff(mut x: impl ZTransport) {
    let mut tcp = TcpStream::connect("127.0.0.1:7447").unwrap();

    // When a call to `unframed` or `framed` would exceed a batch size, it will
    // first perform nothing and return `Performed::Blocked`. The next call will
    // start a new batch automatically.
    x.set_mode(TransportMode::Blocking);
    if x.max_batches() < 2 {
        panic!("Transport must support at least 2 batches for this test");
    }

    while !matches!(
        x.unframed(&InitSyn::default()).unwrap(),
        Performed::Blocked(3)
    ) {}

    // This should call exactly 3 times the inner closure to flush all data.
    x.flush(|data| {
        tcp.write_all(data).unwrap();
    });

    // When a call to `unframed` or `framed` would exceed a batch size, it will
    // start a new batch automatically.
    x.set_mode(TransportMode::StartNewBatch);
    if x.max_batches() < 3 {
        panic!("Transport must support at least 3 batches for this test");
    }

    while !matches!(
        x.unframed(&InitSyn::default()).unwrap(),
        Performed::StartedNewBatch(4)
    ) {}

    // This should call exactly 4 times the inner closure to flush all data. 3 with
    // "full" batches and 1 with a partial batch.
    x.flush(|data| {
        tcp.write_all(data).unwrap();
    });
}

fn main() {
    let mut ht = HeaplessTransport::<16, 8, 2>::new(true);
    ht.set_mode(TransportMode::Blocking);

    assert!(ht.unframed(&[1, 2, 3]).is_ok());
    assert!(ht.unframed(&[4, 5, 6]).is_ok());
    assert!(ht.unframed(&[7, 8, 9]) == Ok(Performed::Blocked(1)));
    assert!(ht.unframed(&[7, 8, 9]).is_ok());
    assert!(ht.unframed(&[10, 11, 12]).is_ok());
    assert!(ht.unframed(&[10, 11, 12]) == Err(TransportError::TransportIsFull));

    println!("Total: {:?}", ht.inner());
    ht.flush(|data| {
        println!("Flushed: {:?}", data);
    });
}

struct HeaplessTransport<const BUFF: usize, const BATCH_SIZE: usize, const MAX_BATCHES: usize> {
    buff: [u8; BUFF],
    pos: usize,
    streamed: bool,
    mode: TransportMode,
    commited: heapless::Vec<usize, MAX_BATCHES>,
}

impl<const BUFF: usize, const BATCH_SIZE: usize, const MAX_BATCHES: usize>
    HeaplessTransport<BUFF, BATCH_SIZE, MAX_BATCHES>
{
    pub fn new(streamed: bool) -> Self {
        const {
            assert!(
                BATCH_SIZE * MAX_BATCHES <= BUFF,
                "Buffer size must be at least batch_size * max_batches"
            );
        }

        Self {
            buff: [0; BUFF],
            pos: if streamed { 2 } else { 0 },
            streamed,
            mode: TransportMode::StartNewBatch,
            commited: heapless::Vec::new(),
        }
    }

    fn inner(&self) -> &[u8] {
        &self.buff[..self.pos]
    }
}

impl<const BUFF: usize, const BATCH_SIZE: usize, const MAX_BATCHES: usize> ZTransport
    for HeaplessTransport<BUFF, BATCH_SIZE, MAX_BATCHES>
{
    fn set_mode(&mut self, mode: TransportMode) {
        self.mode = mode;
    }

    fn batch_size(&self) -> usize {
        BATCH_SIZE
    }

    fn max_batches(&self) -> usize {
        MAX_BATCHES
    }

    fn unframed(&mut self, x: &impl ZEncode) -> core::result::Result<Performed, TransportError> {
        let current_is_last = self.commited.len() + 1 >= MAX_BATCHES;
        let last = self.commited.iter().sum::<usize>();
        let window = self.pos..((last + BATCH_SIZE).min(BUFF));
        let len = window.len();
        let mut buff = &mut self.buff[window];
        if ZEncode::z_encode(x, &mut buff).is_err() {
            if current_is_last {
                return Err(TransportError::TransportIsFull);
            }

            match self.mode {
                TransportMode::Blocking => {
                    self.commited
                        .push(self.pos - last)
                        .expect("Max batches checked above");
                    self.pos += if self.streamed { 2 } else { 0 };

                    return Ok(Performed::Blocked(self.commited.len()));
                }
                TransportMode::StartNewBatch => {
                    self.commited
                        .push(self.pos - last)
                        .expect("Max batches checked above");
                    let last = self.pos;
                    self.pos += if self.streamed { 2 } else { 0 };

                    let window = self.pos..((last + BATCH_SIZE).min(BUFF));
                    let len = window.len();
                    let mut buff = &mut self.buff[window];
                    ZEncode::z_encode(x, &mut buff)
                        .map_err(|_| TransportError::MessageTooLargeForBatch)?;
                    self.pos += len - buff.len();

                    return Ok(Performed::StartedNewBatch(self.commited.len() + 1));
                }
            }
        } else {
            self.pos += len - buff.len();
            return Ok(Performed::Nothing);
        }
    }

    fn flush(&mut self, mut f: impl FnMut(&[u8])) {
        if self.streamed {
            // commited contains the size of each batch. In streamed mode a batch starts
            // with 2 bytes for the length. We need to update those 2 bytes before flushing.
            let mut idx = 0;
            for batch_size in &self.commited {
                let l = (*batch_size - 2) as u16;
                self.buff[idx..idx + 2].copy_from_slice(&l.to_le_bytes());
                f(&self.buff[idx..idx + *batch_size]);
                idx += *batch_size;
            }
            // If there is any remaining data in the current batch, flush it as well.
            if self.pos > idx {
                let l = (self.pos - idx - 2) as u16;
                self.buff[idx..idx + 2].copy_from_slice(&l.to_le_bytes());
                f(&self.buff[idx..self.pos]);
            }
        } else {
            // commited contains the size of each batch.
            let mut idx = 0;
            for batch_size in &self.commited {
                f(&self.buff[idx..idx + *batch_size]);
                idx += *batch_size;
            }
            // If there is any remaining data in the current batch, flush it as well.
            if self.pos > idx {
                f(&self.buff[idx..self.pos]);
            }
        }
        self.pos = if self.streamed { 2 } else { 0 };
        self.commited.clear();
    }
}
