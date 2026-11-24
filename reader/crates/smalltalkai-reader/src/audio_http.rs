use anyhow::{ensure, Result};
use reader_core::processing::ProcessingMode;
use serde_json::Value;
use std::io::Write;

#[derive(Default)]
pub(crate) struct Processing {
    pub mode: Option<ProcessingMode>,
    pub stream: bool,
}

impl Processing {
    pub(crate) fn new(mode: Option<&str>, stream: bool) -> Result<Self> {
        match mode {
            Some("streaming") => Ok(Self {
                mode: Some(ProcessingMode::Chunked),
                stream: true,
            }),
            mode => Ok(Self {
                mode: mode.map(str::parse).transpose()?,
                stream,
            }),
        }
    }
}

/// tiny_http's ordinary response buffers short SSE frames in its chunked
/// encoder. Flush each complete event so a client can consume it immediately.
pub(crate) struct Stream(Box<dyn Write + Send>);

impl Stream {
    pub(crate) fn start(request: tiny_http::Request, content_type: &'static str) -> Result<Self> {
        ensure!(
            request.http_version() >= &tiny_http::HTTPVersion(1, 1),
            "streaming requires HTTP/1.1"
        );
        let mut writer = request.into_writer();
        write!(
            writer,
            "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nTransfer-Encoding: chunked\r\nCache-Control: no-cache\r\n\r\n"
        )?;
        writer.flush()?;
        Ok(Self(writer))
    }

    pub(crate) fn send(&mut self, data: &[u8]) -> Result<()> {
        if !data.is_empty() {
            write!(self.0, "{:x}\r\n", data.len())?;
            self.0.write_all(data)?;
            self.0.write_all(b"\r\n")?;
            self.0.flush()?;
        }
        Ok(())
    }

    pub(crate) fn event(&mut self, event: Value) -> Result<()> {
        self.send(format!("data: {event}\n\n").as_bytes())
    }

    pub(crate) fn finish(mut self) -> Result<()> {
        self.0.write_all(b"0\r\n\r\n")?;
        self.0.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read};
    use std::net::TcpStream;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn http_client_receives_a_small_delta_before_the_stream_finishes() -> Result<()> {
        let server =
            tiny_http::Server::http("127.0.0.1:0").map_err(|error| anyhow::anyhow!("{error}"))?;
        let address = server.server_addr().to_ip().unwrap();
        let (finish, continue_request) = mpsc::channel();
        let sender = std::thread::spawn(move || -> Result<()> {
            let request = server.recv_timeout(Duration::from_secs(4))?.unwrap();
            let mut stream = Stream::start(request, "text/event-stream")?;
            stream.event(serde_json::json!({"type":"transcript.text.delta","delta":"first"}))?;
            continue_request.recv_timeout(Duration::from_secs(4))?;
            stream.finish()
        });
        let mut socket = TcpStream::connect(address)?;
        socket.set_read_timeout(Some(Duration::from_secs(4)))?;
        socket.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")?;
        let mut reply = BufReader::new(socket);
        let mut line = String::new();
        loop {
            line.clear();
            ensure!(reply.read_line(&mut line)? > 0, "missing response header");
            if line == "\r\n" {
                break;
            }
        }
        line.clear();
        reply.read_line(&mut line)?;
        let mut chunk = vec![0; usize::from_str_radix(line.trim(), 16)?];
        reply.read_exact(&mut chunk)?;
        assert_eq!(
            std::str::from_utf8(&chunk)?,
            "data: {\"delta\":\"first\",\"type\":\"transcript.text.delta\"}\n\n"
        );
        finish.send(())?;
        sender.join().unwrap()?;
        Ok(())
    }
}
