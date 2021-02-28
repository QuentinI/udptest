use std::{marker::PhantomData, net::UdpSocket};
///! This module provides traits and types for sending and receiving
///! arbitrary data capable of presenting itself as a buffer of bytes
///! through UDP.
use std::{net::SocketAddr, time::Duration};

use log::warn;

const UDP_MAX_PAYLOAD: usize = 508;
type UdpPayload = [u8; UDP_MAX_PAYLOAD];

#[derive(Debug)]
pub enum Error<T> {
    Io(std::io::Error),
    ParseError(T),
}

pub trait FromUdp: Sized {
    type Error;
    fn from_udp(buf: &[u8]) -> Result<Self, Self::Error>;
}

pub trait FromUdpSource: Sized {
    type Error;
    fn from_udp_source(buf: &[u8], source: SocketAddr) -> Result<Self, Self::Error>;
}

impl<T> FromUdpSource for T
where
    T: FromUdp,
{
    type Error = T::Error;
    fn from_udp_source(buf: &[u8], _: SocketAddr) -> Result<T, T::Error> {
        T::from_udp(buf)
    }
}

pub trait ToUdp {
    fn to_udp(&self) -> Vec<u8>;
}

pub struct Receiver<T> {
    sock: UdpSocket,
    buf: UdpPayload,
    phantom: PhantomData<T>,
}

impl<T> Receiver<T> {
    pub fn new<A: std::net::ToSocketAddrs>(addr: A) -> std::io::Result<Self> {
        let sock = UdpSocket::bind(addr)?;
        sock.set_read_timeout(Some(Duration::from_millis(100)))?;
        Ok(Self {
            sock,
            buf: [0_u8; UDP_MAX_PAYLOAD],
            phantom: PhantomData,
        })
    }
}

impl<T> Iterator for Receiver<T>
where
    T: FromUdpSource,
{
    type Item = Result<T, Error<T::Error>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.sock.recv_from(&mut self.buf) {
            Ok((len, src)) => {
                let val =
                    T::from_udp_source(&self.buf[..len], src).map_err(|e| Error::ParseError(e));
                Some(val)
            }
            Err(e) => Some(Err(Error::Io(e))),
        }
    }
}

pub struct Sender {
    sock: UdpSocket,
}

impl<'a> Sender {
    pub fn new<A>(addr: A) -> std::io::Result<Self>
    where
        A: std::net::ToSocketAddrs,
    {
        Ok(Self {
            sock: UdpSocket::bind(addr)?,
        })
    }

    pub fn send<I, T: 'a, A>(&mut self, iter: I, dest: A) -> std::io::Result<()>
    where
        I: Iterator<Item = &'a T>,
        T: ToUdp,
        A: std::net::ToSocketAddrs,
    {
        self.sock.connect(dest)?;
        for item in iter {
            let item = item.to_udp();
            if item.len() > UDP_MAX_PAYLOAD {
                warn!("Item too large, truncated");
                self.sock.send(&item[..UDP_MAX_PAYLOAD])?;
            } else {
                self.sock.send(&item)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::udp::*;
    use rand::{thread_rng, Rng};
    use std::thread;

    type DummyData = Vec<u8>;

    impl FromUdp for DummyData {
        type Error = ();
        fn from_udp(buf: &[u8]) -> Result<Self, Self::Error> {
            Ok(buf.into())
        }
    }

    impl ToUdp for DummyData {
        fn to_udp(&self) -> Vec<u8> {
            self.clone()
        }
    }

    fn construct_dummy_data() -> Vec<DummyData> {
        let mut rng = thread_rng();
        vec![
            vec![],
            vec![42, 1],
            vec![0; UDP_MAX_PAYLOAD],
            vec![u8::MAX; UDP_MAX_PAYLOAD],
            (0..UDP_MAX_PAYLOAD).map(|_| rng.gen()).collect(),
        ]
    }

    #[test]
    // Basic Sender test
    fn sender() {
        let rx_sock = UdpSocket::bind("0.0.0.0:8567").unwrap();
        let mut sender = Sender::new("0.0.0.0:8568").unwrap();

        let data = construct_dummy_data();
        let copy = data.clone();

        let _t = thread::spawn(move || {
            sender.send(copy.iter(), "127.0.0.1:8567").unwrap();
        });

        for packet in data.iter() {
            let mut buf = vec![0_u8; UDP_MAX_PAYLOAD];
            let (len, _) = rx_sock.recv_from(&mut buf).unwrap();
            assert_eq!(packet, &buf[..len]);
        }
    }

    #[test]
    // Basic receiver test
    fn receiver() {
        let mut receiver: Receiver<DummyData> = Receiver::new("0.0.0.0:8569").unwrap();
        let tx_sock = UdpSocket::bind("0.0.0.0:8570").unwrap();
        tx_sock.connect("127.0.0.1:8569").unwrap();

        let data = construct_dummy_data();
        let copy = data.clone();

        let _t = thread::spawn(move || {
            for packet in copy.iter() {
                tx_sock.send(packet).unwrap();
            }
        });

        for packet in data.iter() {
            let recv = receiver.next().unwrap().unwrap();
            assert_eq!(packet, &recv);
        }
    }
}
