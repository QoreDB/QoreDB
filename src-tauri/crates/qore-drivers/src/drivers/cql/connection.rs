// SPDX-License-Identifier: Apache-2.0

//! One CQL connection: TCP or TLS, the STARTUP handshake, and strict
//! request/response exchanges.
//!
//! Every exchange runs on stream id 0 under `&mut self`. A cluster driver
//! multiplexes hundreds of in-flight requests over one socket; an interactive
//! client sends one statement and waits for its grid to fill, so the borrow
//! checker enforcing one-at-a-time is the whole concurrency design.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use qore_core::error::{EngineError, EngineResult};
use qore_core::types::Value;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;

use super::frame::{
    Consistency, ERR_UNPREPARED, HEADER_LEN, Opcode, Reader, Writer, decode_error, decode_header,
    encode_header, error_code,
};
use super::value::{self, CqlType};

pub trait CqlStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> CqlStream for T {}

#[derive(Clone, Debug, Default)]
pub struct TlsOptions {
    pub enabled: bool,
    /// PEM bundle to trust on top of nothing else. Cassandra clusters routinely
    /// run an internal CA, and there is no permissive fallback: without this the
    /// system trust store must already vouch for the node.
    pub ca_cert_path: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CqlColumn {
    pub name: String,
    pub ty: CqlType,
}

#[derive(Debug, Default)]
pub struct CqlRows {
    pub columns: Vec<CqlColumn>,
    pub rows: Vec<Vec<Value>>,
    /// Opaque cursor. Handed back verbatim on the next request; its contents are
    /// the server's business.
    pub paging_state: Option<Vec<u8>>,
}

#[derive(Debug)]
pub enum CqlResult {
    Void,
    Rows(CqlRows),
    SetKeyspace(String),
    Prepared(Prepared),
    SchemaChange,
}

impl CqlResult {
    pub fn into_rows(self) -> CqlRows {
        match self {
            CqlResult::Rows(rows) => rows,
            _ => CqlRows::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Prepared {
    pub id: Vec<u8>,
    /// Types of the bind markers, in order. The caller encodes against these
    /// rather than guessing from the Rust value.
    pub bind_types: Vec<CqlType>,
}

pub struct CqlConnection {
    stream: Box<dyn CqlStream>,
    io_timeout: Duration,
    prepared: HashMap<String, Prepared>,
    keyspace: Option<String>,
}

impl CqlConnection {
    pub async fn connect(
        host: &str,
        port: u16,
        username: &str,
        password: &str,
        tls: &TlsOptions,
        connect_timeout: Duration,
        io_timeout: Duration,
    ) -> EngineResult<Self> {
        let tcp = tokio::time::timeout(connect_timeout, TcpStream::connect((host, port)))
            .await
            .map_err(|_| {
                EngineError::connection_failed(format!(
                    "Timed out connecting to {host}:{port} after {}s",
                    connect_timeout.as_secs()
                ))
            })?
            .map_err(|e| EngineError::connection_failed(format!("{host}:{port}: {e}")))?;
        tcp.set_nodelay(true).ok();

        let stream: Box<dyn CqlStream> = if tls.enabled {
            Box::new(upgrade_to_tls(tcp, host, tls).await?)
        } else {
            Box::new(tcp)
        };

        let mut conn = Self {
            stream,
            io_timeout,
            prepared: HashMap::new(),
            keyspace: None,
        };
        conn.startup(username, password).await?;
        Ok(conn)
    }

    pub fn keyspace(&self) -> Option<&str> {
        self.keyspace.as_deref()
    }

    /// A connection whose peer half is already dropped. Lets the driver's
    /// policy tests construct a session without a server; any I/O on it fails,
    /// which is correct for code paths that must not reach the wire.
    #[cfg(test)]
    pub fn for_tests() -> Self {
        let (client, _server) = tokio::io::duplex(64);
        Self {
            stream: Box::new(client),
            io_timeout: Duration::from_secs(1),
            prepared: HashMap::new(),
            keyspace: None,
        }
    }

    async fn startup(&mut self, username: &str, password: &str) -> EngineResult<()> {
        let mut w = Writer::new();
        w.string_map(&[("CQL_VERSION", "3.0.0")]);
        let (opcode, body) = self.call(Opcode::Startup, w.finish()).await?;
        match opcode {
            Opcode::Ready => Ok(()),
            Opcode::Authenticate => {
                let authenticator = Reader::new(&body).string().unwrap_or_default();
                if username.is_empty() {
                    return Err(EngineError::auth_failed(format!(
                        "Server requires {authenticator} but no username was supplied"
                    )));
                }
                self.authenticate(username, password).await
            }
            Opcode::Error => Err(decode_error(&body)),
            other => Err(EngineError::internal(format!(
                "Unexpected {other:?} in answer to STARTUP"
            ))),
        }
    }

    /// `PasswordAuthenticator` is SASL PLAIN: an empty authzid, then the
    /// username and password, all NUL-separated.
    async fn authenticate(&mut self, username: &str, password: &str) -> EngineResult<()> {
        let mut token = Vec::with_capacity(username.len() + password.len() + 2);
        token.push(0);
        token.extend_from_slice(username.as_bytes());
        token.push(0);
        token.extend_from_slice(password.as_bytes());

        let mut w = Writer::new();
        w.bytes(&token);
        let (opcode, body) = self.call(Opcode::AuthResponse, w.finish()).await?;
        match opcode {
            Opcode::AuthSuccess => Ok(()),
            Opcode::Error => Err(decode_error(&body)),
            Opcode::AuthChallenge => Err(EngineError::auth_failed(
                "Server asked for a multi-step SASL exchange, which this client does not implement",
            )),
            other => Err(EngineError::internal(format!(
                "Unexpected {other:?} in answer to AUTH_RESPONSE"
            ))),
        }
    }

    async fn call(&mut self, opcode: Opcode, body: Vec<u8>) -> EngineResult<(Opcode, Vec<u8>)> {
        let frame = encode_header(opcode, 0, &body);
        tokio::time::timeout(self.io_timeout, self.stream.write_all(&frame))
            .await
            .map_err(|_| EngineError::connection_failed("Timed out writing a CQL frame"))?
            .map_err(|e| EngineError::connection_failed(format!("CQL write failed: {e}")))?;
        tokio::time::timeout(self.io_timeout, self.stream.flush())
            .await
            .map_err(|_| EngineError::connection_failed("Timed out flushing a CQL frame"))?
            .map_err(|e| EngineError::connection_failed(format!("CQL flush failed: {e}")))?;

        let mut header = [0u8; HEADER_LEN];
        tokio::time::timeout(self.io_timeout, self.stream.read_exact(&mut header))
            .await
            .map_err(|_| EngineError::connection_failed("Timed out reading a CQL header"))?
            .map_err(|e| EngineError::connection_failed(format!("CQL read failed: {e}")))?;
        let header = decode_header(&header)?;

        let mut body = vec![0u8; header.length];
        if header.length > 0 {
            tokio::time::timeout(self.io_timeout, self.stream.read_exact(&mut body))
                .await
                .map_err(|_| EngineError::connection_failed("Timed out reading a CQL body"))?
                .map_err(|e| EngineError::connection_failed(format!("CQL read failed: {e}")))?;
        }
        Ok((header.opcode, body))
    }

    pub async fn use_keyspace(&mut self, keyspace: &str) -> EngineResult<()> {
        let quoted = quote_identifier(keyspace);
        self.query(&format!("USE {quoted}"), None, None).await?;
        self.keyspace = Some(keyspace.to_string());
        // Statements prepared against the previous keyspace resolve unqualified
        // names differently; their ids are no longer the ones we want.
        self.prepared.clear();
        Ok(())
    }

    pub async fn query(
        &mut self,
        cql: &str,
        page_size: Option<i32>,
        paging_state: Option<&[u8]>,
    ) -> EngineResult<CqlResult> {
        let mut w = Writer::new();
        w.long_string(cql);
        write_query_params(&mut w, &[], page_size, paging_state);
        let (opcode, body) = self.call(Opcode::Query, w.finish()).await?;
        self.interpret(opcode, body)
    }

    pub async fn prepare(&mut self, cql: &str) -> EngineResult<Prepared> {
        if let Some(cached) = self.prepared.get(cql) {
            return Ok(cached.clone());
        }
        let prepared = self.prepare_uncached(cql).await?;
        self.prepared.insert(cql.to_string(), prepared.clone());
        Ok(prepared)
    }

    async fn prepare_uncached(&mut self, cql: &str) -> EngineResult<Prepared> {
        let mut w = Writer::new();
        w.long_string(cql);
        let (opcode, body) = self.call(Opcode::Prepare, w.finish()).await?;
        match self.interpret(opcode, body)? {
            CqlResult::Prepared(prepared) => Ok(prepared),
            other => Err(EngineError::internal(format!(
                "PREPARE answered with {other:?} instead of a statement id"
            ))),
        }
    }

    /// Binds and runs a prepared statement. A node that has forgotten the id —
    /// after a restart or a schema change — answers `UNPREPARED`; re-preparing
    /// once and retrying is the documented recovery, not an error to surface.
    pub async fn execute(
        &mut self,
        cql: &str,
        values: &[Value],
        page_size: Option<i32>,
        paging_state: Option<&[u8]>,
    ) -> EngineResult<CqlResult> {
        let prepared = self.prepare(cql).await?;
        match self
            .execute_prepared(&prepared, values, page_size, paging_state)
            .await
        {
            Err(EngineError::Internal { message }) if message == UNPREPARED_MARKER => {
                self.prepared.remove(cql);
                let prepared = self.prepare(cql).await?;
                self.execute_prepared(&prepared, values, page_size, paging_state)
                    .await
            }
            other => other,
        }
    }

    async fn execute_prepared(
        &mut self,
        prepared: &Prepared,
        values: &[Value],
        page_size: Option<i32>,
        paging_state: Option<&[u8]>,
    ) -> EngineResult<CqlResult> {
        if values.len() != prepared.bind_types.len() {
            return Err(EngineError::validation(format!(
                "Statement expects {} bound value(s), got {}",
                prepared.bind_types.len(),
                values.len()
            )));
        }
        let encoded: Vec<Option<Vec<u8>>> = prepared
            .bind_types
            .iter()
            .zip(values)
            .map(|(ty, v)| value::encode(ty, v))
            .collect::<EngineResult<_>>()?;

        let mut w = Writer::new();
        w.short_bytes(&prepared.id);
        write_query_params(&mut w, &encoded, page_size, paging_state);
        let (opcode, body) = self.call(Opcode::Execute, w.finish()).await?;
        if opcode == Opcode::Error && error_code(&body) == Some(ERR_UNPREPARED) {
            return Err(EngineError::internal(UNPREPARED_MARKER));
        }
        self.interpret(opcode, body)
    }

    fn interpret(&self, opcode: Opcode, body: Vec<u8>) -> EngineResult<CqlResult> {
        match opcode {
            Opcode::Result => parse_result(&body),
            Opcode::Error => Err(decode_error(&body)),
            other => Err(EngineError::internal(format!(
                "Unexpected {other:?} where a RESULT was due"
            ))),
        }
    }
}

/// Internal sentinel for "the node forgot this prepared id". It never leaves
/// `execute`, which retries on it.
const UNPREPARED_MARKER: &str = "__cql_unprepared__";

fn write_query_params(
    w: &mut Writer,
    values: &[Option<Vec<u8>>],
    page_size: Option<i32>,
    paging_state: Option<&[u8]>,
) {
    w.consistency(Consistency::LocalOne);
    let mut flags = 0u8;
    if !values.is_empty() {
        flags |= 0x01;
    }
    if page_size.is_some() {
        flags |= 0x04;
    }
    if paging_state.is_some() {
        flags |= 0x08;
    }
    w.u8(flags);
    if !values.is_empty() {
        w.u16(values.len() as u16);
        for v in values {
            match v {
                Some(bytes) => w.bytes(bytes),
                None => w.i32(-1),
            }
        }
    }
    if let Some(size) = page_size {
        w.i32(size);
    }
    if let Some(state) = paging_state {
        w.bytes(state);
    }
}

fn parse_result(body: &[u8]) -> EngineResult<CqlResult> {
    let mut r = Reader::new(body);
    Ok(match r.i32()? {
        0x0001 => CqlResult::Void,
        0x0002 => CqlResult::Rows(parse_rows(&mut r)?),
        0x0003 => CqlResult::SetKeyspace(r.string()?),
        0x0004 => CqlResult::Prepared(parse_prepared(&mut r)?),
        0x0005 => CqlResult::SchemaChange,
        kind => {
            return Err(EngineError::internal(format!(
                "Unknown CQL result kind {kind}"
            )));
        }
    })
}

const FLAG_GLOBAL_TABLES_SPEC: i32 = 0x0001;
const FLAG_HAS_MORE_PAGES: i32 = 0x0002;
const FLAG_NO_METADATA: i32 = 0x0004;

struct RowMetadata {
    columns: Vec<CqlColumn>,
    paging_state: Option<Vec<u8>>,
}

/// `<flags><columns_count>[<paging_state>][<global_table_spec>?<col_spec>*]`.
/// The paging state sits before the table spec, not after it.
fn read_row_metadata(r: &mut Reader<'_>) -> EngineResult<RowMetadata> {
    let flags = r.i32()?;
    let count = r.i32()?;
    if count < 0 {
        return Err(EngineError::internal("Negative CQL column count"));
    }

    let paging_state = if flags & FLAG_HAS_MORE_PAGES != 0 {
        r.bytes()?.map(<[u8]>::to_vec)
    } else {
        None
    };

    if flags & FLAG_NO_METADATA != 0 {
        return Ok(RowMetadata {
            columns: Vec::new(),
            paging_state,
        });
    }

    let global = flags & FLAG_GLOBAL_TABLES_SPEC != 0;
    if global {
        let _keyspace = r.string()?;
        let _table = r.string()?;
    }
    let mut columns = Vec::with_capacity(count as usize);
    for _ in 0..count {
        if !global {
            let _keyspace = r.string()?;
            let _table = r.string()?;
        }
        let name = r.string()?;
        columns.push(CqlColumn {
            name,
            ty: value::read_type(r)?,
        });
    }
    Ok(RowMetadata {
        columns,
        paging_state,
    })
}

fn parse_rows(r: &mut Reader<'_>) -> EngineResult<CqlRows> {
    let meta = read_row_metadata(r)?;
    let row_count = r.i32()?;
    if row_count < 0 {
        return Err(EngineError::internal("Negative CQL row count"));
    }

    let mut rows = Vec::with_capacity(row_count.min(10_000) as usize);
    for _ in 0..row_count {
        let mut row = Vec::with_capacity(meta.columns.len());
        for column in &meta.columns {
            row.push(value::decode(&column.ty, r.bytes()?)?);
        }
        rows.push(row);
    }

    Ok(CqlRows {
        columns: meta.columns,
        rows,
        paging_state: meta.paging_state,
    })
}

/// `<id><metadata><result_metadata>`, where the prepared metadata carries a
/// partition-key index list that v4 added and we skip over.
fn parse_prepared(r: &mut Reader<'_>) -> EngineResult<Prepared> {
    let id = r.short_bytes()?.to_vec();

    let flags = r.i32()?;
    let count = r.i32()?;
    if count < 0 {
        return Err(EngineError::internal("Negative CQL bind marker count"));
    }
    let pk_count = r.i32()?;
    for _ in 0..pk_count.max(0) {
        let _pk_index = r.u16()?;
    }

    let global = flags & FLAG_GLOBAL_TABLES_SPEC != 0;
    if global {
        let _keyspace = r.string()?;
        let _table = r.string()?;
    }
    let mut bind_types = Vec::with_capacity(count as usize);
    for _ in 0..count {
        if !global {
            let _keyspace = r.string()?;
            let _table = r.string()?;
        }
        let _name = r.string()?;
        bind_types.push(value::read_type(r)?);
    }

    Ok(Prepared { id, bind_types })
}

/// CQL identifiers are case-folded unless double-quoted; an embedded quote is
/// escaped by doubling it. Used for keyspace and table names coming from the
/// catalog, which is also where a name chosen to break out of a statement would
/// arrive from.
pub fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

async fn upgrade_to_tls(
    tcp: TcpStream,
    host: &str,
    tls: &TlsOptions,
) -> EngineResult<tokio_rustls::client::TlsStream<TcpStream>> {
    use tokio_rustls::TlsConnector;
    use tokio_rustls::rustls::pki_types::ServerName;
    use tokio_rustls::rustls::{ClientConfig, RootCertStore};

    let mut roots = RootCertStore::empty();
    match &tls.ca_cert_path {
        Some(path) => {
            let pem = std::fs::read(path).map_err(|e| EngineError::SslError {
                message: format!("Cannot read CA bundle `{path}`: {e}"),
            })?;
            let mut cursor = std::io::Cursor::new(pem);
            let certs: Vec<_> = rustls_pemfile::certs(&mut cursor)
                .collect::<Result<_, _>>()
                .map_err(|e| EngineError::SslError {
                    message: format!("Cannot parse CA bundle `{path}`: {e}"),
                })?;
            if certs.is_empty() {
                return Err(EngineError::SslError {
                    message: format!("CA bundle `{path}` holds no certificate"),
                });
            }
            for cert in certs {
                roots.add(cert).map_err(|e| EngineError::SslError {
                    message: format!("Rejected a certificate from `{path}`: {e}"),
                })?;
            }
        }
        None => {
            let native = rustls_native_certs::load_native_certs();
            if native.certs.is_empty() {
                return Err(EngineError::SslError {
                    message: "No system CA certificates found; point the connection at a CA bundle"
                        .to_string(),
                });
            }
            for cert in native.certs {
                let _ = roots.add(cert);
            }
        }
    }

    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let server_name =
        ServerName::try_from(host.to_string()).map_err(|_| EngineError::SslError {
            message: format!("`{host}` is not a valid TLS server name"),
        })?;
    TlsConnector::from(Arc::new(config))
        .connect(server_name, tcp)
        .await
        .map_err(|e| EngineError::SslError {
            message: format!("TLS handshake with {host} failed: {e}"),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::cql::value::CqlType;

    fn rows_body(build: impl FnOnce(&mut Writer)) -> Vec<u8> {
        let mut w = Writer::new();
        w.i32(0x0002); // kind = Rows
        build(&mut w);
        w.finish()
    }

    #[test]
    fn parses_a_global_spec_result_with_two_rows() {
        let body = rows_body(|w| {
            w.i32(FLAG_GLOBAL_TABLES_SPEC);
            w.i32(2); // two columns
            w.string("ks");
            w.string("users");
            w.string("id");
            w.u16(0x0009); // int
            w.string("name");
            w.u16(0x000D); // varchar
            w.i32(2); // two rows
            w.bytes(&1i32.to_be_bytes());
            w.bytes(b"ada");
            w.bytes(&2i32.to_be_bytes());
            w.i32(-1); // NULL name
        });

        let rows = parse_result(&body).expect("parses").into_rows();
        assert_eq!(rows.columns.len(), 2);
        assert_eq!(rows.columns[0].name, "id");
        assert_eq!(rows.columns[1].ty, CqlType::Varchar);
        assert_eq!(rows.rows.len(), 2);
        assert_eq!(format!("{:?}", rows.rows[0][1]), r#"Text("ada")"#);
        assert_eq!(format!("{:?}", rows.rows[1][1]), "Null");
        assert!(rows.paging_state.is_none());
    }

    #[test]
    fn the_paging_state_is_read_before_the_table_spec() {
        // Getting this order wrong parses the cursor as a keyspace name and
        // silently loses every page after the first.
        let body = rows_body(|w| {
            w.i32(FLAG_GLOBAL_TABLES_SPEC | FLAG_HAS_MORE_PAGES);
            w.i32(1);
            w.bytes(b"cursor-blob");
            w.string("ks");
            w.string("users");
            w.string("id");
            w.u16(0x0009);
            w.i32(1);
            w.bytes(&7i32.to_be_bytes());
        });

        let rows = parse_result(&body).expect("parses").into_rows();
        assert_eq!(rows.paging_state.as_deref(), Some(&b"cursor-blob"[..]));
        assert_eq!(rows.columns[0].name, "id");
        assert_eq!(format!("{:?}", rows.rows[0][0]), "Int(7)");
    }

    #[test]
    fn per_column_table_specs_are_read_when_there_is_no_global_one() {
        let body = rows_body(|w| {
            w.i32(0); // no global spec
            w.i32(1);
            w.string("ks");
            w.string("users");
            w.string("id");
            w.u16(0x0009);
            w.i32(0);
        });

        let rows = parse_result(&body).expect("parses").into_rows();
        assert_eq!(rows.columns[0].name, "id");
        assert!(rows.rows.is_empty());
    }

    #[test]
    fn prepared_metadata_skips_the_partition_key_indexes() {
        let mut w = Writer::new();
        w.i32(0x0004); // kind = Prepared
        w.short_bytes(b"\xAB\xCD");
        w.i32(FLAG_GLOBAL_TABLES_SPEC);
        w.i32(2); // two bind markers
        w.i32(1); // one partition key index
        w.u16(0); // ...which we must consume, not read as a string
        w.string("ks");
        w.string("users");
        w.string("id");
        w.u16(0x0009); // int
        w.string("name");
        w.u16(0x000D); // varchar
        let body = w.finish();

        let CqlResult::Prepared(prepared) = parse_result(&body).expect("parses") else {
            panic!("expected a prepared statement");
        };
        assert_eq!(prepared.id, b"\xAB\xCD");
        assert_eq!(prepared.bind_types, vec![CqlType::Int, CqlType::Varchar]);
    }

    #[test]
    fn set_keyspace_and_void_results_are_recognised() {
        let mut w = Writer::new();
        w.i32(0x0003);
        w.string("system");
        assert!(matches!(
            parse_result(&w.finish()).unwrap(),
            CqlResult::SetKeyspace(ks) if ks == "system"
        ));

        let mut w = Writer::new();
        w.i32(0x0001);
        assert!(matches!(
            parse_result(&w.finish()).unwrap(),
            CqlResult::Void
        ));
    }

    #[test]
    fn query_params_set_the_flag_for_each_option_supplied() {
        let mut w = Writer::new();
        write_query_params(&mut w, &[], None, None);
        let buf = w.finish();
        assert_eq!(buf[2], 0x00, "no values, no page size, no cursor");

        let mut w = Writer::new();
        write_query_params(&mut w, &[Some(vec![1])], Some(100), Some(b"c"));
        let buf = w.finish();
        assert_eq!(buf[2], 0x01 | 0x04 | 0x08);
    }

    #[test]
    fn a_bound_null_is_written_as_a_negative_length() {
        let mut w = Writer::new();
        write_query_params(&mut w, &[None], None, None);
        let buf = w.finish();
        // consistency (2) + flags (1) + count (2) + the -1 length (4)
        assert_eq!(&buf[5..9], &(-1i32).to_be_bytes());
    }

    #[test]
    fn identifiers_are_quoted_and_embedded_quotes_doubled() {
        assert_eq!(quote_identifier("users"), "\"users\"");
        assert_eq!(quote_identifier("we\"ird"), "\"we\"\"ird\"");
    }

    #[test]
    fn a_truncated_result_body_errors_instead_of_panicking() {
        let body = rows_body(|w| {
            w.i32(FLAG_GLOBAL_TABLES_SPEC);
            w.i32(1);
            w.string("ks");
            w.string("users");
            w.string("id");
            w.u16(0x0009);
            w.i32(5); // claims five rows, supplies none
        });
        assert!(parse_result(&body).is_err());
    }
}
