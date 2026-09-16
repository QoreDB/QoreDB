// SPDX-License-Identifier: Apache-2.0

//! Wire primitives and framing for the CQL binary protocol v4.
//!
//! Written by hand rather than delegated to a cluster driver: a desktop client
//! runs one statement at a time over one connection, so token-aware routing,
//! load balancing and speculative execution would be paid for and never used.

use qore_core::error::{EngineError, EngineResult};

pub const PROTOCOL_VERSION: u8 = 0x04;
const RESPONSE_FLAG: u8 = 0x80;
pub const HEADER_LEN: usize = 9;

/// A server may advertise frames up to 256 MB. We refuse far earlier: the grid
/// never renders that much, and a corrupt `length` would otherwise have us
/// allocate on garbage before the first byte of body has arrived.
pub const MAX_FRAME_LEN: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opcode {
    Error,
    Startup,
    Ready,
    Authenticate,
    Options,
    Supported,
    Query,
    Result,
    Prepare,
    Execute,
    Register,
    Event,
    Batch,
    AuthChallenge,
    AuthResponse,
    AuthSuccess,
}

impl Opcode {
    pub fn as_u8(self) -> u8 {
        match self {
            Opcode::Error => 0x00,
            Opcode::Startup => 0x01,
            Opcode::Ready => 0x02,
            Opcode::Authenticate => 0x03,
            Opcode::Options => 0x05,
            Opcode::Supported => 0x06,
            Opcode::Query => 0x07,
            Opcode::Result => 0x08,
            Opcode::Prepare => 0x09,
            Opcode::Execute => 0x0A,
            Opcode::Register => 0x0B,
            Opcode::Event => 0x0C,
            Opcode::Batch => 0x0D,
            Opcode::AuthChallenge => 0x0E,
            Opcode::AuthResponse => 0x0F,
            Opcode::AuthSuccess => 0x10,
        }
    }

    pub fn from_u8(value: u8) -> EngineResult<Self> {
        Ok(match value {
            0x00 => Opcode::Error,
            0x01 => Opcode::Startup,
            0x02 => Opcode::Ready,
            0x03 => Opcode::Authenticate,
            0x05 => Opcode::Options,
            0x06 => Opcode::Supported,
            0x07 => Opcode::Query,
            0x08 => Opcode::Result,
            0x09 => Opcode::Prepare,
            0x0A => Opcode::Execute,
            0x0B => Opcode::Register,
            0x0C => Opcode::Event,
            0x0D => Opcode::Batch,
            0x0E => Opcode::AuthChallenge,
            0x0F => Opcode::AuthResponse,
            0x10 => Opcode::AuthSuccess,
            other => {
                return Err(EngineError::internal(format!(
                    "Unknown CQL opcode 0x{other:02X}"
                )));
            }
        })
    }
}

/// Consistency levels. Only the ones a read-mostly client needs are exposed;
/// `LocalOne` is the default because it keeps a multi-DC cluster from paying
/// cross-datacenter latency on every browse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Consistency {
    One,
    Quorum,
    All,
    LocalQuorum,
    LocalOne,
}

impl Consistency {
    pub fn as_u16(self) -> u16 {
        match self {
            Consistency::One => 0x0001,
            Consistency::Quorum => 0x0004,
            Consistency::All => 0x0005,
            Consistency::LocalQuorum => 0x0006,
            Consistency::LocalOne => 0x000A,
        }
    }
}

pub fn encode_header(opcode: Opcode, stream: i16, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + body.len());
    out.push(PROTOCOL_VERSION);
    out.push(0); // no compression, no tracing
    out.extend_from_slice(&stream.to_be_bytes());
    out.push(opcode.as_u8());
    out.extend_from_slice(&(body.len() as i32).to_be_bytes());
    out.extend_from_slice(body);
    out
}

const FLAG_COMPRESSED: u8 = 0x01;
const FLAG_TRACING: u8 = 0x02;
const FLAG_CUSTOM_PAYLOAD: u8 = 0x04;
const FLAG_WARNING: u8 = 0x08;

#[derive(Debug, Clone, Copy)]
pub struct Header {
    pub flags: u8,
    pub opcode: Opcode,
    pub stream: i16,
    pub length: usize,
}

pub fn decode_header(bytes: &[u8; HEADER_LEN]) -> EngineResult<Header> {
    let version = bytes[0];
    if version & RESPONSE_FLAG == 0 {
        return Err(EngineError::internal(format!(
            "Expected a CQL response frame, got version byte 0x{version:02X}"
        )));
    }
    if version & !RESPONSE_FLAG != PROTOCOL_VERSION {
        return Err(EngineError::connection_failed(format!(
            "Server answered with CQL protocol v{}, this client speaks v{PROTOCOL_VERSION}",
            version & !RESPONSE_FLAG
        )));
    }
    let flags = bytes[1];
    if flags & FLAG_COMPRESSED != 0 {
        return Err(EngineError::internal(
            "Server sent a compressed CQL frame although compression was never negotiated",
        ));
    }
    let stream = i16::from_be_bytes([bytes[2], bytes[3]]);
    let opcode = Opcode::from_u8(bytes[4])?;
    let length = i32::from_be_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]);
    if length < 0 || length as usize > MAX_FRAME_LEN {
        return Err(EngineError::internal(format!(
            "CQL frame announces {length} bytes, refusing to allocate"
        )));
    }
    Ok(Header {
        flags,
        opcode,
        stream,
        length: length as usize,
    })
}

/// A server may prefix the body with a tracing id, a list of warnings and a
/// custom payload, in that order, each announced by a header flag. None of
/// them changes the meaning of the message, but the real body starts after
/// them. ScyllaDB sets the warning flag on `CREATE KEYSPACE`, for instance.
pub fn strip_body_prefix(flags: u8, body: &[u8]) -> EngineResult<&[u8]> {
    let mut r = Reader::new(body);
    if flags & FLAG_TRACING != 0 {
        r.take(16)?;
    }
    if flags & FLAG_WARNING != 0 {
        r.string_list()?;
    }
    if flags & FLAG_CUSTOM_PAYLOAD != 0 {
        let n = r.u16()? as usize;
        for _ in 0..n {
            r.string()?;
            r.bytes()?;
        }
    }
    Ok(&body[body.len() - r.remaining()..])
}

/// Cursor over a response body. Every read is bounds-checked: a truncated or
/// malformed frame must surface as an error, never as a panic in the UI thread.
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    fn take(&mut self, n: usize) -> EngineResult<&'a [u8]> {
        if self.remaining() < n {
            return Err(EngineError::internal(format!(
                "Truncated CQL frame: wanted {n} bytes, {} left",
                self.remaining()
            )));
        }
        let slice = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    pub fn u8(&mut self) -> EngineResult<u8> {
        Ok(self.take(1)?[0])
    }

    pub fn i16(&mut self) -> EngineResult<i16> {
        let b = self.take(2)?;
        Ok(i16::from_be_bytes([b[0], b[1]]))
    }

    pub fn u16(&mut self) -> EngineResult<u16> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    pub fn i32(&mut self) -> EngineResult<i32> {
        let b = self.take(4)?;
        Ok(i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn string(&mut self) -> EngineResult<String> {
        let n = self.u16()? as usize;
        let bytes = self.take(n)?;
        String::from_utf8(bytes.to_vec())
            .map_err(|_| EngineError::internal("CQL [string] is not valid UTF-8"))
    }

    pub fn long_string(&mut self) -> EngineResult<String> {
        let n = self.i32()?;
        if n < 0 {
            return Err(EngineError::internal("Negative CQL [long string] length"));
        }
        let bytes = self.take(n as usize)?;
        String::from_utf8(bytes.to_vec())
            .map_err(|_| EngineError::internal("CQL [long string] is not valid UTF-8"))
    }

    /// `[bytes]`: a negative length means SQL NULL, which is distinct from an
    /// empty blob and must not collapse into one.
    pub fn bytes(&mut self) -> EngineResult<Option<&'a [u8]>> {
        let n = self.i32()?;
        if n < 0 {
            return Ok(None);
        }
        Ok(Some(self.take(n as usize)?))
    }

    pub fn short_bytes(&mut self) -> EngineResult<&'a [u8]> {
        let n = self.u16()? as usize;
        self.take(n)
    }

    pub fn string_list(&mut self) -> EngineResult<Vec<String>> {
        let n = self.u16()? as usize;
        (0..n).map(|_| self.string()).collect()
    }

    pub fn string_multimap(&mut self) -> EngineResult<Vec<(String, Vec<String>)>> {
        let n = self.u16()? as usize;
        (0..n)
            .map(|_| Ok((self.string()?, self.string_list()?)))
            .collect()
    }
}

pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    pub fn finish(self) -> Vec<u8> {
        self.buf
    }

    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub fn i32(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub fn string(&mut self, v: &str) {
        self.u16(v.len() as u16);
        self.buf.extend_from_slice(v.as_bytes());
    }

    pub fn long_string(&mut self, v: &str) {
        self.i32(v.len() as i32);
        self.buf.extend_from_slice(v.as_bytes());
    }

    pub fn bytes(&mut self, v: &[u8]) {
        self.i32(v.len() as i32);
        self.buf.extend_from_slice(v);
    }

    pub fn short_bytes(&mut self, v: &[u8]) {
        self.u16(v.len() as u16);
        self.buf.extend_from_slice(v);
    }

    pub fn consistency(&mut self, v: Consistency) {
        self.u16(v.as_u16());
    }

    pub fn string_map(&mut self, entries: &[(&str, &str)]) {
        self.u16(entries.len() as u16);
        for (k, v) in entries {
            self.string(k);
            self.string(v);
        }
    }
}

impl Default for Writer {
    fn default() -> Self {
        Self::new()
    }
}

/// Decodes an ERROR frame into the closest `EngineError` variant, so the UI can
/// tell a bad password from a syntax mistake without string-matching.
pub fn decode_error(body: &[u8]) -> EngineError {
    let mut r = Reader::new(body);
    let code = match r.i32() {
        Ok(c) => c,
        Err(e) => return e,
    };
    let message = r.string().unwrap_or_else(|_| "unknown error".to_string());
    match code {
        0x0100 => EngineError::auth_failed(message),
        0x2000 => EngineError::syntax_error(message),
        0x2100 => EngineError::auth_failed(message),
        0x2200 | 0x2300 | 0x2400 => EngineError::validation(message),
        0x1000..=0x1002 => EngineError::connection_failed(message),
        _ => EngineError::execution_error(message),
    }
}

/// Server-side error code for "the prepared statement id is unknown here",
/// which happens after a schema change or a node restart. The caller re-prepares
/// instead of surfacing it.
pub const ERR_UNPREPARED: i32 = 0x2500;

pub fn error_code(body: &[u8]) -> Option<i32> {
    Reader::new(body).i32().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_round_trips() {
        let body = b"hello".to_vec();
        let frame = encode_header(Opcode::Query, 7, &body);
        assert_eq!(frame[0], PROTOCOL_VERSION);
        assert_eq!(frame.len(), HEADER_LEN + body.len());

        // Flip the version bit the way a server response would.
        let mut raw = [0u8; HEADER_LEN];
        raw.copy_from_slice(&frame[..HEADER_LEN]);
        raw[0] |= RESPONSE_FLAG;
        let header = decode_header(&raw).expect("header decodes");
        assert_eq!(header.opcode, Opcode::Query);
        assert_eq!(header.stream, 7);
        assert_eq!(header.length, body.len());
    }

    #[test]
    fn flagged_prefixes_are_skipped_in_protocol_order() {
        let mut w = Writer::new();
        w.string("a warning");
        let mut prefixed = [0x11u8; 16].to_vec();
        prefixed.extend_from_slice(&1u16.to_be_bytes());
        prefixed.extend_from_slice(&w.finish());
        prefixed.extend_from_slice(&1u16.to_be_bytes());
        prefixed.extend_from_slice(&3u16.to_be_bytes());
        prefixed.extend_from_slice(b"key");
        prefixed.extend_from_slice(&2i32.to_be_bytes());
        prefixed.extend_from_slice(&[9, 9]);
        prefixed.extend_from_slice(&[0xAA, 0xBB]);

        let flags = FLAG_TRACING | FLAG_WARNING | FLAG_CUSTOM_PAYLOAD;
        assert_eq!(strip_body_prefix(flags, &prefixed).unwrap(), &[0xAA, 0xBB]);
        assert_eq!(strip_body_prefix(0, &[0xAA]).unwrap(), &[0xAA]);
        assert!(strip_body_prefix(FLAG_WARNING, &[0x00]).is_err());
    }

    #[test]
    fn a_compressed_frame_is_refused() {
        let mut raw = [0u8; HEADER_LEN];
        raw[0] = RESPONSE_FLAG | PROTOCOL_VERSION;
        raw[1] = FLAG_COMPRESSED;
        raw[4] = Opcode::Ready.as_u8();
        assert!(decode_header(&raw).is_err());
    }

    #[test]
    fn a_request_frame_is_not_accepted_as_a_response() {
        let frame = encode_header(Opcode::Ready, 0, &[]);
        let mut raw = [0u8; HEADER_LEN];
        raw.copy_from_slice(&frame[..HEADER_LEN]);
        assert!(decode_header(&raw).is_err());
    }

    #[test]
    fn a_foreign_protocol_version_is_rejected() {
        let mut raw = [0u8; HEADER_LEN];
        raw[0] = RESPONSE_FLAG | 0x03;
        raw[4] = Opcode::Ready.as_u8();
        let err = decode_header(&raw).expect_err("v3 must be refused");
        assert!(err.to_string().contains("v3"), "{err}");
    }

    #[test]
    fn an_oversized_length_is_refused_before_allocating() {
        let mut raw = [0u8; HEADER_LEN];
        raw[0] = RESPONSE_FLAG | PROTOCOL_VERSION;
        raw[4] = Opcode::Result.as_u8();
        raw[5..9].copy_from_slice(&(i32::MAX).to_be_bytes());
        assert!(decode_header(&raw).is_err());
    }

    #[test]
    fn primitives_round_trip() {
        let mut w = Writer::new();
        w.string("keyspace");
        w.long_string("SELECT * FROM t");
        w.bytes(b"\x01\x02");
        w.short_bytes(b"\xAA");
        w.i32(-7);
        let buf = w.finish();

        let mut r = Reader::new(&buf);
        assert_eq!(r.string().unwrap(), "keyspace");
        assert_eq!(r.long_string().unwrap(), "SELECT * FROM t");
        assert_eq!(r.bytes().unwrap(), Some(&b"\x01\x02"[..]));
        assert_eq!(r.short_bytes().unwrap(), b"\xAA");
        assert_eq!(r.i32().unwrap(), -7);
        assert_eq!(r.remaining(), 0);
    }

    #[test]
    fn a_null_value_is_distinct_from_an_empty_blob() {
        let mut w = Writer::new();
        w.i32(-1);
        w.bytes(&[]);
        let buf = w.finish();

        let mut r = Reader::new(&buf);
        assert_eq!(r.bytes().unwrap(), None, "-1 is NULL");
        assert_eq!(r.bytes().unwrap(), Some(&[][..]), "0 is an empty blob");
    }

    #[test]
    fn reading_past_the_end_errors_instead_of_panicking() {
        let mut r = Reader::new(&[0x00, 0x05]);
        assert!(r.string().is_err());
    }

    #[test]
    fn error_frames_map_to_engine_errors() {
        let mut w = Writer::new();
        w.i32(0x2000);
        w.string("line 1:0 no viable alternative");
        let syntax = decode_error(&w.finish());
        assert!(
            matches!(syntax, EngineError::SyntaxError { .. }),
            "{syntax:?}"
        );

        let mut w = Writer::new();
        w.i32(0x0100);
        w.string("Provided username is not correct");
        let auth = decode_error(&w.finish());
        assert!(
            matches!(auth, EngineError::AuthenticationFailed { .. }),
            "{auth:?}"
        );
    }
}
