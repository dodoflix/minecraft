//! Vanilla connection pipeline below the packet layer: `Varint21` framing, zlib compression and
//! AES-128/CFB8 encryption (`Varint21FrameDecoder`, `Varint21LengthFieldPrepender`,
//! `CompressionDecoder/Encoder`, `CipherDecoder/Encoder`), plus tokio helpers for both ends.
//!
//! Payloads are opaque `VarInt packet id + body` byte strings. Any [`NetError`] other than
//! `Io`/`Eof` is what vanilla would disconnect on; the affected half must not be used afterwards.
//!
//! As in vanilla's netty pipeline, `set_compression`/`enable_encryption` only affect data processed
//! afterwards: on the read side, bytes already pushed into the decoder are not decrypted retroactively.

use crate::codec::{read_varint, varint_size, write_varint, DecodeError};
use aes::Aes128;
use cfb8::cipher::KeyIvInit;
use flate2::{Compress, Compression, Decompress, FlushCompress, FlushDecompress, Status};
use tokio::io::{
    split, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadHalf, WriteHalf,
};

/// `CompressionDecoder.MAXIMUM_UNCOMPRESSED_LENGTH`.
pub const MAXIMUM_UNCOMPRESSED_LENGTH: usize = 8_388_608;
/// Largest frame length expressible in a 3-byte VarInt.
pub const MAXIMUM_FRAME_LENGTH: usize = 2_097_151;

#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// The peer closed the stream.
    #[error("end of stream")]
    Eof,
    /// Netty `CorruptedFrameException`.
    #[error("{0}")]
    CorruptedFrame(&'static str),
    /// Vanilla `DecoderException` (compression decoder).
    #[error("{0}")]
    Decoder(String),
    /// Vanilla `EncoderException` (frame prepender; compression encoder: wrapped `IllegalArgumentException`).
    #[error("{0}")]
    Encoder(String),
    #[error(transparent)]
    Decode(#[from] DecodeError),
}

type Enc = cfb8::Encryptor<Aes128>;
type Dec = cfb8::Decryptor<Aes128>;

/// Write half: `payload -> [compress] -> length prefix -> [encrypt]`.
pub struct FrameEncoder {
    /// `None` = compression off (vanilla: no `compress` handler).
    threshold: Option<usize>,
    deflater: Compress,
    scratch: Vec<u8>,
    cipher: Option<Enc>,
}

impl Default for FrameEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameEncoder {
    pub fn new() -> Self {
        Self {
            threshold: None,
            deflater: Compress::new(Compression::default(), true),
            scratch: Vec::new(),
            cipher: None,
        }
    }

    /// `Connection.setupCompression`: a threshold `>= 0` enables compression, negative disables it.
    pub fn set_compression(&mut self, threshold: i32) {
        self.threshold = usize::try_from(threshold).ok();
    }

    pub fn enable_encryption(&mut self, secret: [u8; 16]) {
        self.cipher = Some(Enc::new(&secret.into(), &secret.into()));
    }

    /// Appends the wire form of `payload` to `out`; `out` is untouched on error.
    pub fn encode(&mut self, payload: &[u8], out: &mut Vec<u8>) -> Result<(), NetError> {
        let start = out.len();
        match self.threshold {
            None => put_frame(out, payload)?,
            Some(threshold) => {
                let len = payload.len();
                if len > MAXIMUM_UNCOMPRESSED_LENGTH {
                    // `CompressionEncoder` throws `IllegalArgumentException`, which `MessageToByteEncoder` wraps.
                    return Err(NetError::Encoder(format!(
                        "java.lang.IllegalArgumentException: Packet too big (is {len}, should be less than {MAXIMUM_UNCOMPRESSED_LENGTH})"
                    )));
                }
                let body = &mut self.scratch;
                body.clear();
                if len < threshold {
                    write_varint(body, 0);
                    body.extend_from_slice(payload);
                } else {
                    // `len <= 8388608` was checked above.
                    write_varint(body, len as i32);
                    self.deflater.reset();
                    loop {
                        if body.capacity() - body.len() < 64 {
                            body.reserve(len.max(4096));
                        }
                        let consumed = self.deflater.total_in() as usize;
                        let status = self
                            .deflater
                            .compress_vec(&payload[consumed..], body, FlushCompress::Finish)
                            .map_err(|e| NetError::Encoder(e.to_string()))?;
                        if status == Status::StreamEnd {
                            break;
                        }
                    }
                }
                put_frame(out, &self.scratch)?;
            }
        }
        if let Some(cipher) = &mut self.cipher {
            cipher.encrypt(&mut out[start..]);
        }
        Ok(())
    }
}

/// `Varint21LengthFieldPrepender`.
fn put_frame(out: &mut Vec<u8>, body: &[u8]) -> Result<(), NetError> {
    let len = body.len();
    if varint_size(len as i32) > 3 {
        return Err(NetError::Encoder(format!("Packet too large: size {len} is over 8")));
    }
    out.reserve(len + 3);
    write_varint(out, len as i32);
    out.extend_from_slice(body);
    Ok(())
}

struct Inflate {
    threshold: i32,
    validate: bool,
    inflater: Decompress,
}

impl Inflate {
    /// `CompressionDecoder.decode` for one frame body.
    fn decode(&mut self, mut frame: &[u8]) -> Result<Vec<u8>, NetError> {
        let declared = read_varint(&mut frame)?;
        if declared == 0 {
            return Ok(frame.to_vec());
        }
        if self.validate && declared < self.threshold {
            return Err(NetError::Decoder(format!(
                "Badly compressed packet - size of {declared} is below server threshold of {}",
                self.threshold
            )));
        }
        // Vanilla only checks the maximum when validating (and netty rejects negatives); never
        // allocate more than the protocol maximum, whatever the peer declares.
        if declared > MAXIMUM_UNCOMPRESSED_LENGTH as i32 {
            return Err(NetError::Decoder(format!(
                "Badly compressed packet - size of {declared} is larger than protocol maximum of {MAXIMUM_UNCOMPRESSED_LENGTH}"
            )));
        }
        if declared < 0 {
            return Err(NetError::Decoder(format!("capacity: {declared} (expected: 0+)")));
        }
        let declared = declared as usize;
        let mut out = vec![0u8; declared];
        self.inflater.reset(true);
        // Like `Inflater.inflate(ByteBuffer)`: stops when the buffer is full (extra data is ignored).
        let mut produced = 0;
        while produced < declared {
            let consumed = self.inflater.total_in() as usize;
            let status = self
                .inflater
                .decompress(&frame[consumed..], &mut out[produced..], FlushDecompress::None)
                .map_err(|e| NetError::Decoder(e.to_string()))?;
            let progressed = self.inflater.total_out() as usize - produced;
            produced += progressed;
            if status == Status::StreamEnd || (progressed == 0 && self.inflater.total_in() as usize == consumed) {
                break;
            }
        }
        if produced != declared {
            return Err(NetError::Decoder(format!(
                "Badly compressed packet - actual length of uncompressed payload {produced} is does not match declared size {declared}"
            )));
        }
        Ok(out)
    }
}

/// Read half: `[decrypt] -> split frames -> [decompress] -> payload`.
pub struct FrameDecoder {
    buf: Vec<u8>,
    /// Start of unconsumed bytes in `buf`.
    pos: usize,
    inflate: Option<Inflate>,
    cipher: Option<Dec>,
}

impl Default for FrameDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self { buf: Vec::new(), pos: 0, inflate: None, cipher: None }
    }

    /// `Connection.setupCompression`: a threshold `>= 0` enables decompression, negative disables it.
    pub fn set_compression(&mut self, threshold: i32, validate: bool) {
        self.inflate = (threshold >= 0).then(|| Inflate {
            threshold,
            validate,
            inflater: Decompress::new(true),
        });
    }

    pub fn enable_encryption(&mut self, secret: [u8; 16]) {
        self.cipher = Some(Dec::new(&secret.into(), &secret.into()));
    }

    /// Feeds raw stream bytes (decrypting them if encryption is on).
    pub fn push(&mut self, bytes: &[u8]) {
        if self.pos == self.buf.len() {
            self.buf.clear();
            self.pos = 0;
        } else if self.pos >= 4096 {
            self.buf.drain(..self.pos);
            self.pos = 0;
        }
        let start = self.buf.len();
        self.buf.extend_from_slice(bytes);
        if let Some(cipher) = &mut self.cipher {
            cipher.decrypt(&mut self.buf[start..]);
        }
    }

    /// The next complete frame's decompressed payload, or `None` if more bytes are needed.
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>, NetError> {
        let avail = &self.buf[self.pos..];
        let mut length = 0usize;
        let mut header = 0;
        for i in 0..3 {
            let Some(&b) = avail.get(i) else { return Ok(None) };
            length |= usize::from(b & 0x7F) << (7 * i);
            if b & 0x80 == 0 {
                header = i + 1;
                break;
            }
        }
        if header == 0 {
            return Err(NetError::CorruptedFrame("length wider than 21-bit"));
        }
        if length == 0 {
            return Err(NetError::CorruptedFrame("Frame length cannot be zero"));
        }
        let Some(frame) = avail[header..].get(..length) else { return Ok(None) };
        let payload = match &mut self.inflate {
            Some(inflate) => inflate.decode(frame)?,
            None => frame.to_vec(),
        };
        self.pos += header + length;
        Ok(Some(payload))
    }
}

const READ_CHUNK: usize = 8192;

pub struct PacketReader<R> {
    inner: R,
    decoder: FrameDecoder,
    chunk: Box<[u8; READ_CHUNK]>,
}

impl<R: AsyncRead + Unpin> PacketReader<R> {
    pub fn new(inner: R) -> Self {
        Self { inner, decoder: FrameDecoder::new(), chunk: Box::new([0; READ_CHUNK]) }
    }

    pub fn set_compression(&mut self, threshold: i32, validate: bool) {
        self.decoder.set_compression(threshold, validate);
    }

    pub fn enable_encryption(&mut self, secret: [u8; 16]) {
        self.decoder.enable_encryption(secret);
    }

    /// Next payload (`VarInt id + body`); `Err(Eof)` when the peer closed the stream, even mid-frame.
    /// Cancel-safe: partial frames stay buffered.
    pub async fn read_payload(&mut self) -> Result<Vec<u8>, NetError> {
        loop {
            if let Some(payload) = self.decoder.next_frame()? {
                return Ok(payload);
            }
            match self.inner.read(&mut self.chunk[..]).await? {
                0 => return Err(NetError::Eof),
                n => self.decoder.push(&self.chunk[..n]),
            }
        }
    }

    pub fn into_inner(self) -> R {
        self.inner
    }
}

pub struct PacketWriter<W> {
    inner: W,
    encoder: FrameEncoder,
    buf: Vec<u8>,
}

impl<W: AsyncWrite + Unpin> PacketWriter<W> {
    pub fn new(inner: W) -> Self {
        Self { inner, encoder: FrameEncoder::new(), buf: Vec::new() }
    }

    pub fn set_compression(&mut self, threshold: i32) {
        self.encoder.set_compression(threshold);
    }

    pub fn enable_encryption(&mut self, secret: [u8; 16]) {
        self.encoder.enable_encryption(secret);
    }

    /// Frames, compresses and encrypts `payload` and writes it (call [`flush`](Self::flush) to push it out).
    pub async fn write_payload(&mut self, payload: &[u8]) -> Result<(), NetError> {
        self.buf.clear();
        self.encoder.encode(payload, &mut self.buf)?;
        self.inner.write_all(&self.buf).await?;
        Ok(())
    }

    pub async fn flush(&mut self) -> Result<(), NetError> {
        Ok(self.inner.flush().await?)
    }

    pub fn into_inner(self) -> W {
        self.inner
    }
}

/// Both halves of one duplex stream.
pub struct Connection<S> {
    reader: PacketReader<ReadHalf<S>>,
    writer: PacketWriter<WriteHalf<S>>,
}

impl<S: AsyncRead + AsyncWrite> Connection<S> {
    pub fn new(stream: S) -> Self {
        let (r, w) = split(stream);
        Self { reader: PacketReader::new(r), writer: PacketWriter::new(w) }
    }

    /// Both directions, like `Connection.setupCompression`.
    pub fn set_compression(&mut self, threshold: i32, validate: bool) {
        self.reader.set_compression(threshold, validate);
        self.writer.set_compression(threshold);
    }

    /// Both directions use the same secret as key and IV, like `Connection.setEncryptionKey`.
    pub fn enable_encryption(&mut self, secret: [u8; 16]) {
        self.reader.enable_encryption(secret);
        self.writer.enable_encryption(secret);
    }

    pub async fn read_payload(&mut self) -> Result<Vec<u8>, NetError> {
        self.reader.read_payload().await
    }

    pub async fn write_payload(&mut self, payload: &[u8]) -> Result<(), NetError> {
        self.writer.write_payload(payload).await
    }

    pub async fn flush(&mut self) -> Result<(), NetError> {
        self.writer.flush().await
    }

    pub fn into_split(self) -> (PacketReader<ReadHalf<S>>, PacketWriter<WriteHalf<S>>) {
        (self.reader, self.writer)
    }
}
