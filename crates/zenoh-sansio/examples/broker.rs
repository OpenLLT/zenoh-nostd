use std::time::Instant;

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use zenoh_sansio::{Broker, Id};

#[tokio::main]
async fn main() {
    // Broker with internal buffer of 2048 bytes (used to encode packets)
    let start = Instant::now();
    let mut broker = Broker::<2048>::new();

    // Connect to gateway
    let mut gateway_stream = TcpStream::connect("127.0.0.1:7447").await.unwrap();
    let init_pckt = broker.init().unwrap();
    write_tcp(&mut gateway_stream, init_pckt.as_ref())
        .await
        .unwrap();

    // Accept left client
    let left = TcpListener::bind("127.0.0.1:7555").await.unwrap();
    let mut left_stream = left.accept().await.unwrap().0;

    // Accept right client
    let right = TcpListener::bind("127.0.0.1:7556").await.unwrap();
    let mut right_stream = right.accept().await.unwrap().0;

    loop {
        let mut rx = [0u8; 2048];
        // Read from any of the three connections (uses tokio::select!)
        let (id, data) = try_read_tcp3(
            &mut gateway_stream,
            &mut left_stream,
            &mut right_stream,
            &mut rx,
        )
        .await
        .unwrap();

        // Process received data
        let packets = broker.recv::<16>(data, id).unwrap();

        // Dispatch packets to their destination
        for packet in packets {
            match packet.dst() {
                Id::North => write_tcp(&mut gateway_stream, packet.as_ref())
                    .await
                    .unwrap(),
                Id::South(1) => write_tcp(&mut left_stream, packet.as_ref()).await.unwrap(),
                Id::South(2) => write_tcp(&mut right_stream, packet.as_ref()).await.unwrap(),
                _ => {}
            }
        }
    }
}

async fn try_read_tcp3<'b>(
    stream1: &mut TcpStream,
    stream2: &mut TcpStream,
    stream3: &mut TcpStream,
    buf: &'b mut [u8],
) -> std::io::Result<(Id, &'b [u8])> {
    let mut l1 = [0u8; 2];
    let mut l2 = [0u8; 2];
    let mut l3 = [0u8; 2];

    tokio::select! {
        _ = stream1.read_exact(&mut l1) => {
            let l = u16::from_le_bytes(l1) as usize;
            stream1.read_exact(&mut buf[..l]).await?;
            Ok((Id::North, &buf[..l]))
        }
        _ = stream2.read_exact(&mut l2) => {
            let l = u16::from_le_bytes(l2) as usize;
            stream2.read_exact(&mut buf[..l]).await?;
            Ok((Id::South(1), &buf[..l]))
        }
        _ = stream3.read_exact(&mut l3) => {
            let l = u16::from_le_bytes(l3) as usize;
            stream3.read_exact(&mut buf[..l]).await?;
            Ok((Id::South(2), &buf[..l]))
        }
    }
}

async fn write_tcp(stream: &mut TcpStream, data: &[u8]) -> std::io::Result<()> {
    let l = data.len() as u16;
    stream.write_all(&l.to_le_bytes()).await?;
    stream.write_all(data).await?;
    Ok(())
}
