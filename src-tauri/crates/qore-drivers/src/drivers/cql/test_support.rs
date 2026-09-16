// SPDX-License-Identifier: Apache-2.0

use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

use super::frame::{HEADER_LEN, Opcode, Reader, encode_header};

pub async fn read_query(peer: &mut DuplexStream) -> String {
    let mut header = [0; HEADER_LEN];
    peer.read_exact(&mut header).await.unwrap();
    assert_eq!(header[4], Opcode::Query.as_u8());
    let len = u32::from_be_bytes(header[5..9].try_into().unwrap()) as usize;
    let mut body = vec![0; len];
    peer.read_exact(&mut body).await.unwrap();
    Reader::new(&body).long_string().unwrap()
}

pub fn response(opcode: Opcode, body: &[u8]) -> Vec<u8> {
    let mut frame = encode_header(opcode, 0, body);
    frame[0] = 0x84;
    frame
}

pub async fn reply_void(peer: &mut DuplexStream) {
    peer.write_all(&response(Opcode::Result, &1i32.to_be_bytes()))
        .await
        .unwrap();
}
