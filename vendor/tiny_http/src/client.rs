use ascii::AsciiString;

use std::io::Error as IoError;
use std::io::Result as IoResult;
use std::io::{BufReader, BufWriter, ErrorKind, Read};

use std::net::SocketAddr;
use std::str::FromStr;

use crate::common::{HTTPVersion, Method};
use crate::util::RefinedTcpStream;
use crate::util::{SequentialReader, SequentialReaderBuilder, SequentialWriterBuilder};
use crate::Request;

/// A ClientConnection is an object that will store a socket to a client
/// and return Request objects.
pub struct ClientConnection {
    // address of the client
    remote_addr: IoResult<Option<SocketAddr>>,

    // sequence of Readers to the stream, so that the data is not read in
    //  the wrong order
    source: SequentialReaderBuilder<BufReader<RefinedTcpStream>>,

    // sequence of Writers to the stream, to avoid writing response #2 before
    //  response #1
    sink: SequentialWriterBuilder<BufWriter<RefinedTcpStream>>,

    // Reader to read the next header from
    next_header_source: SequentialReader<BufReader<RefinedTcpStream>>,

    // set to true if we know that the previous request is the last one
    no_more_requests: bool,

    // true if the connection goes through SSL
    secure: bool,
}

/// Error that can happen when reading a request.
#[derive(Debug)]
enum ReadError {
    WrongRequestLine,
    WrongHeader(HTTPVersion),
    UnsupportedVersion,
    PayloadTooLarge(HTTPVersion),
    /// the client sent an unrecognized `Expect` header
    ExpectationFailed(HTTPVersion),
    ReadIoError(IoError),
}

impl ClientConnection {
    /// Creates a new `ClientConnection` that takes ownership of the `TcpStream`.
    pub fn new(
        write_socket: RefinedTcpStream,
        mut read_socket: RefinedTcpStream,
    ) -> ClientConnection {
        let remote_addr = read_socket.peer_addr();
        let secure = read_socket.secure();

        let mut source = SequentialReaderBuilder::new(BufReader::with_capacity(1024, read_socket));
        let first_header = source.next().unwrap();

        ClientConnection {
            source,
            sink: SequentialWriterBuilder::new(BufWriter::with_capacity(1024, write_socket)),
            remote_addr,
            next_header_source: first_header,
            no_more_requests: false,
            secure,
        }
    }

    /// true if the connection is HTTPS
    /// Reads the next line from self.next_header_source.
    ///
    /// Reads until `CRLF` is reached. The next read will start
    ///  at the first byte of the new line.
    fn read_next_line(&mut self, deadline: std::time::Instant) -> IoResult<AsciiString> {
        let mut buf = Vec::new();
        let mut prev_byte_was_cr = false;

        loop {
            if std::time::Instant::now() >= deadline {
                return Err(IoError::new(
                    ErrorKind::TimedOut,
                    "HTTP header deadline exceeded",
                ));
            }
            let byte = self.next_header_source.by_ref().bytes().next();

            let byte = match byte {
                Some(b) => b?,
                None => return Err(IoError::new(ErrorKind::ConnectionAborted, "Unexpected EOF")),
            };

            if byte == b'\n' && prev_byte_was_cr {
                buf.pop(); // removing the '\r'
                if buf
                    .iter()
                    .any(|byte| *byte < b' ' && *byte != b'\t' || *byte == 127)
                {
                    return Err(IoError::new(
                        ErrorKind::InvalidInput,
                        "Invalid HTTP header character",
                    ));
                }
                return AsciiString::from_ascii(buf)
                    .map_err(|_| IoError::new(ErrorKind::InvalidInput, "Header is not in ASCII"));
            }

            prev_byte_was_cr = byte == b'\r';

            if buf.len() >= 8192 {
                return Err(IoError::new(
                    ErrorKind::InvalidInput,
                    "HTTP header line too long",
                ));
            }
            buf.push(byte);
        }
    }

    /// Reads a request from the stream.
    /// Blocks until the header has been read.
    fn read(&mut self) -> Result<Request, ReadError> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let (method, path, version, headers) = {
            // reading the request line
            let (method, path, version) = {
                let line = self
                    .read_next_line(deadline)
                    .map_err(ReadError::ReadIoError)?;

                parse_request_line(
                    line.as_str().trim(), // TODO: remove this conversion
                )?
            };

            if version > (1, 1) {
                return Err(ReadError::UnsupportedVersion);
            }
            // getting all headers
            let headers = {
                let mut headers = Vec::new();
                let mut header_bytes = 0;
                loop {
                    let line = self
                        .read_next_line(deadline)
                        .map_err(ReadError::ReadIoError)?;

                    if line.is_empty() {
                        break;
                    };
                    header_bytes += line.len() + 2;
                    if headers.len() >= 96 || header_bytes > 32768 {
                        return Err(ReadError::WrongHeader(version));
                    }
                    headers.push(match FromStr::from_str(line.as_str().trim()) {
                        // TODO: remove this conversion
                        Ok(h) => h,
                        _ => return Err(ReadError::WrongHeader(version)),
                    });
                }

                headers
            };

            (method, path, version, headers)
        };

        // building the writer for the request
        let writer = self.sink.next().unwrap();

        // follow-up for next potential request
        let mut data_source = self.source.next().unwrap();
        std::mem::swap(&mut self.next_header_source, &mut data_source);

        // building the next reader
        let request = crate::request::new_request(
            self.secure,
            method,
            path,
            version.clone(),
            headers,
            *self.remote_addr.as_ref().unwrap(),
            data_source,
            writer,
        )
        .map_err(|e| {
            use crate::request;
            match e {
                request::RequestCreationError::PayloadTooLarge => {
                    ReadError::PayloadTooLarge(version)
                }
                request::RequestCreationError::CreationIoError(e) => ReadError::ReadIoError(e),
                request::RequestCreationError::ExpectationFailed => {
                    ReadError::ExpectationFailed(version)
                }
            }
        })?;

        // return the request
        Ok(request)
    }
}

impl Iterator for ClientConnection {
    type Item = Request;

    /// Blocks until the next Request is available.
    /// Returns None when no new Requests will come from the client.
    fn next(&mut self) -> Option<Request> {
        use crate::{Response, StatusCode};
        if self.no_more_requests {
            return None;
        }
        // Rejected bodies are discarded on response. A later request must
        // use a separate connection.
        self.no_more_requests = true;
        let (status, version) = match self.read() {
            Ok(request) => return Some(request),
            Err(ReadError::WrongRequestLine) => (400, HTTPVersion(1, 1)),
            Err(ReadError::WrongHeader(version)) => (400, version),
            Err(ReadError::UnsupportedVersion) => (505, HTTPVersion(1, 1)),
            Err(ReadError::PayloadTooLarge(version)) => (413, version),
            Err(ReadError::ExpectationFailed(version)) => (417, version),
            Err(ReadError::ReadIoError(error)) => match error.kind() {
                ErrorKind::TimedOut | ErrorKind::WouldBlock => (408, HTTPVersion(1, 1)),
                _ => return None,
            },
        };
        let writer = self.sink.next().unwrap();
        let response = Response::new_empty(StatusCode(status));
        let _ = response.raw_print(writer, version, &[], false, None);
        None
    }
}

/// Parses a "HTTP/1.1" string.
fn parse_http_version(version: &str) -> Result<HTTPVersion, ReadError> {
    let (major, minor) = match version {
        "HTTP/0.9" => (0, 9),
        "HTTP/1.0" => (1, 0),
        "HTTP/1.1" => (1, 1),
        "HTTP/2.0" => (2, 0),
        "HTTP/3.0" => (3, 0),
        _ => return Err(ReadError::WrongRequestLine),
    };

    Ok(HTTPVersion(major, minor))
}

/// Parses the request line of the request.
/// eg. GET / HTTP/1.1
fn parse_request_line(line: &str) -> Result<(Method, String, HTTPVersion), ReadError> {
    let mut parts = line.split(' ');

    let method = parts.next().and_then(|w| w.parse().ok());
    let path = parts.next().map(ToOwned::to_owned);
    let version = parts.next().and_then(|w| parse_http_version(w).ok());

    method
        .and_then(|method| Some((method, path?, version?)))
        .ok_or(ReadError::WrongRequestLine)
}

#[cfg(test)]
mod test {
    #[test]
    fn test_parse_request_line() {
        let (method, path, ver) = super::parse_request_line("GET /hello HTTP/1.1").unwrap();

        assert!(method == crate::Method::Get);
        assert!(path == "/hello");
        assert!(ver == crate::common::HTTPVersion(1, 1));

        assert!(super::parse_request_line("GET /hello").is_err());
        assert!(super::parse_request_line("qsd qsd qsd").is_err());
    }
}
