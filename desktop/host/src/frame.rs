//! The framing of the core-to-host protocol:
//! `[channel: u32 LE][kind: u8][length: u32 LE][payload]`. Channel 0
//! carries JSON-RPC 2.0, one message per DATA frame; the other channels
//! carry terminal streams.

use std::io::{self, Read, Write};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// The largest payload; a larger frame closes the connection.
pub const MAX_PAYLOAD: usize = 4 * 1024 * 1024;

/// The channel of the JSON-RPC messages.
pub const CONTROL_CHANNEL: u32 = 0;

/// The kinds of frame.
pub mod kind {
    pub const DATA: u8 = 0;
    pub const CREDIT: u8 = 1;
    pub const CLOSE: u8 = 2;
    pub const RESIZE: u8 = 3;
    pub const CONTROL: u8 = 4;
    pub const FACTS: u8 = 5;
    pub const META: u8 = 6;
    pub const FOCUS: u8 = 7;
    pub const REPLY: u8 = 8;
}

const HEADER: usize = 9;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub channel: u32,
    pub kind: u8,
    pub payload: Vec<u8>,
}

impl Frame {
    /// A JSON-RPC message on channel 0.
    pub fn json(message: &serde_json::Value) -> Self {
        Self {
            channel: CONTROL_CHANNEL,
            kind: kind::DATA,
            payload: message.to_string().into_bytes(),
        }
    }

    /// The frame as it goes on the wire.
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(HEADER + self.payload.len());
        bytes.extend(self.channel.to_le_bytes());
        bytes.push(self.kind);
        bytes.extend((self.payload.len() as u32).to_le_bytes());
        bytes.extend(&self.payload);
        bytes
    }
}

fn header(bytes: [u8; HEADER]) -> io::Result<(u32, u8, usize)> {
    let channel = u32::from_le_bytes(bytes[0..4].try_into().expect("4 bytes"));
    let length = u32::from_le_bytes(bytes[5..9].try_into().expect("4 bytes")) as usize;
    if length > MAX_PAYLOAD {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("a frame of {length} bytes is over the limit"),
        ));
    }
    Ok((channel, bytes[4], length))
}

/// Reads the next frame, or `None` when the other side closed the
/// connection between frames.
pub async fn read<R: AsyncRead + Unpin>(reader: &mut R) -> io::Result<Option<Frame>> {
    let mut bytes = [0u8; HEADER];
    match reader.read_exact(&mut bytes).await {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let (channel, kind, length) = header(bytes)?;
    let mut payload = vec![0u8; length];
    reader.read_exact(&mut payload).await?;
    Ok(Some(Frame {
        channel,
        kind,
        payload,
    }))
}

/// Reads frames, keeping a partly read frame between calls, so that a
/// call dropped while it waits, as a branch of `tokio::select!` that
/// another branch beat, loses no bytes: each wait is one `read`, which
/// takes nothing from the stream unless it completes.
pub struct FrameReader<R> {
    reader: R,
    buf: Vec<u8>,
}

impl<R: AsyncRead + Unpin> FrameReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buf: Vec::new(),
        }
    }

    /// The next frame, or `None` when the stream ends between frames.
    pub async fn next(&mut self) -> io::Result<Option<Frame>> {
        loop {
            if let Some(frame) = self.complete()? {
                return Ok(Some(frame));
            }
            let mut chunk = [0u8; 64 * 1024];
            let n = self.reader.read(&mut chunk).await?;
            if n == 0 {
                return if self.buf.is_empty() {
                    Ok(None)
                } else {
                    Err(io::ErrorKind::UnexpectedEof.into())
                };
            }
            self.buf.extend_from_slice(&chunk[..n]);
        }
    }

    /// A whole frame from the bytes read so far, if they hold one.
    fn complete(&mut self) -> io::Result<Option<Frame>> {
        let Some(head) = self.buf.get(..HEADER) else {
            return Ok(None);
        };
        let (channel, kind, length) = header(head.try_into().expect("HEADER bytes"))?;
        if self.buf.len() < HEADER + length {
            return Ok(None);
        }
        let payload = self.buf[HEADER..HEADER + length].to_vec();
        self.buf.drain(..HEADER + length);
        Ok(Some(Frame {
            channel,
            kind,
            payload,
        }))
    }
}

pub async fn write<W: AsyncWrite + Unpin>(writer: &mut W, frame: &Frame) -> io::Result<()> {
    writer.write_all(&frame.encode()).await?;
    writer.flush().await
}

/// [`read`], for a blocking stream.
pub fn read_blocking<R: Read>(reader: &mut R) -> io::Result<Option<Frame>> {
    let mut bytes = [0u8; HEADER];
    match reader.read_exact(&mut bytes) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let (channel, kind, length) = header(bytes)?;
    let mut payload = vec![0u8; length];
    reader.read_exact(&mut payload)?;
    Ok(Some(Frame {
        channel,
        kind,
        payload,
    }))
}

/// [`write`], for a blocking stream.
pub fn write_blocking<W: Write>(writer: &mut W, frame: &Frame) -> io::Result<()> {
    writer.write_all(&frame.encode())?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_reads_back_as_written() {
        let frame = Frame {
            channel: 7,
            kind: kind::RESIZE,
            payload: vec![80, 0, 24, 0],
        };
        let bytes = frame.encode();
        assert_eq!(&bytes[..9], &[7, 0, 0, 0, 3, 4, 0, 0, 0]);
        let mut reader = &bytes[..];
        assert_eq!(read_blocking(&mut reader).unwrap(), Some(frame));
        assert_eq!(read_blocking(&mut reader).unwrap(), None);
    }

    #[test]
    fn a_frame_over_the_limit_is_refused() {
        let mut bytes = vec![0, 0, 0, 0, kind::DATA];
        bytes.extend(((MAX_PAYLOAD + 1) as u32).to_le_bytes());
        let error = read_blocking(&mut &bytes[..]).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
}
