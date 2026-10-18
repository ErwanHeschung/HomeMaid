//! Minimal HTTP/1.1 POST over the local network: one request per connection, `Content-Length` bodies.
//! ponytail: no chunked encoding or keep-alive, local devices don't need them.

use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::Duration,
};

/// Bounds every socket call: the host cannot interrupt a plugin blocked in one.
const TIMEOUT: Duration = Duration::from_secs(5);

pub struct Response {
    pub status: u16,
    /// Every `name=value` pair of the `Set-Cookie` headers, attributes included.
    pub cookies: Vec<(String, String)>,
    pub body: Vec<u8>,
}

pub fn post(addr: SocketAddr, path: &str, body: &[u8], headers: &[(&str, &str)]) -> Result<Response, String> {
    let io = |e: std::io::Error| format!("{addr}{path}: {e}");
    let mut stream = TcpStream::connect_timeout(&addr, TIMEOUT).map_err(io)?;
    stream.set_read_timeout(Some(TIMEOUT)).map_err(io)?;
    stream.set_write_timeout(Some(TIMEOUT)).map_err(io)?;

    let mut head = format!(
        "POST {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        head += &format!("{name}: {value}\r\n");
    }
    head += "\r\n";
    stream.write_all(head.as_bytes()).map_err(io)?;
    stream.write_all(body).map_err(io)?;

    // Read until the announced body is complete, so a device keeping the connection open doesn't stall us.
    let mut raw = Vec::new();
    let mut chunk = [0; 4096];
    loop {
        if let Some(response) = parse(&raw)? {
            return Ok(response);
        }
        match stream.read(&mut chunk).map_err(io)? {
            0 => return parse(&raw)?.ok_or_else(|| format!("{addr}{path}: connection closed mid-response")),
            n => raw.extend_from_slice(&chunk[..n]),
        }
    }
}

/// `None` while the response is incomplete.
fn parse(raw: &[u8]) -> Result<Option<Response>, String> {
    let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") else { return Ok(None) };
    let head = std::str::from_utf8(&raw[..end]).map_err(|_| "invalid HTTP headers")?;
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .and_then(|code| code.parse().ok())
        .ok_or("invalid HTTP status line")?;
    let mut cookies = Vec::new();
    let mut length = 0;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        if name.eq_ignore_ascii_case("set-cookie") {
            cookies.extend(value.split(';').filter_map(|pair| {
                let (k, v) = pair.split_once('=')?;
                Some((k.trim().to_string(), v.trim().to_string()))
            }));
        } else if name.eq_ignore_ascii_case("content-length") {
            length = value.trim().parse().map_err(|_| "invalid Content-Length")?;
        }
    }
    let body = &raw[end + 4..];
    if body.len() < length {
        return Ok(None);
    }
    Ok(Some(Response { status, cookies, body: body[..length].to_vec() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_complete_responses_only() {
        let raw = b"HTTP/1.1 200 OK\r\nSet-Cookie: TP_SESSIONID=ABC;TIMEOUT=86400\r\nContent-Length: 4\r\n\r\nbody";
        assert!(parse(&raw[..raw.len() - 1]).unwrap().is_none());
        let response = parse(raw).unwrap().unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"body");
        assert_eq!(
            response.cookies,
            [("TP_SESSIONID".into(), "ABC".into()), ("TIMEOUT".into(), "86400".into())]
        );
        assert!(parse(b"HTTP/1.1 403 Forbidden\r\n\r\n").unwrap().is_some_and(|r| r.status == 403 && r.body.is_empty()));
    }
}
