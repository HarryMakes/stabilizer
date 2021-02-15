use core::fmt::Write;
use heapless::{consts::*, String, Vec};
use serde::{Deserialize, Serialize};
use serde_json_core::{de::from_slice, ser::to_string};
use smoltcp as net;

use dsp::iir;

#[macro_export]
macro_rules! route_request {
    ($request:ident,
            readable_attributes: [$($read_attribute:tt: $getter:tt),*],
            modifiable_attributes: [$($write_attribute:tt: $TYPE:ty, $setter:tt),*]) => {
        match $request.req {
            server::AccessRequest::Read => {
                match $request.attribute {
                $(
                    $read_attribute => {
                        #[allow(clippy::redundant_closure_call)]
                        let value = match $getter() {
                            Ok(data) => data,
                            Err(_) => return server::Response::error($request.attribute,
                                                                     "Failed to read attribute"),
                        };

                        let encoded_data: String<U256> = match serde_json_core::to_string(&value) {
                            Ok(data) => data,
                            Err(_) => return server::Response::error($request.attribute,
                                    "Failed to encode attribute value"),
                        };

                        server::Response::success($request.attribute, &encoded_data)
                    },
                 )*
                    _ => server::Response::error($request.attribute, "Unknown attribute")
                }
            },
            server::AccessRequest::Write => {
                match $request.attribute {
                $(
                    $write_attribute => {
                        let new_value = match serde_json_core::from_str::<$TYPE>(&$request.value) {
                            Ok((data, _)) => data,
                            Err(_) => return server::Response::error($request.attribute,
                                    "Failed to decode value"),
                        };

                        #[allow(clippy::redundant_closure_call)]
                        match $setter(new_value) {
                            Ok(_) => server::Response::success($request.attribute, &$request.value),
                            Err(_) => server::Response::error($request.attribute,
                                    "Failed to set attribute"),
                        }
                    }
                 )*
                    _ => server::Response::error($request.attribute, "Unknown attribute")
                }
            }
        }
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub enum AccessRequest {
    Read,
    Write,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Request<'a> {
    pub req: AccessRequest,
    pub attribute: &'a str,
    pub value: String<U256>,
}

#[derive(Serialize, Deserialize)]
pub struct IirRequest {
    pub channel: u8,
    pub iir: iir::IIR,
}

#[derive(Serialize)]
pub struct Response {
    code: i32,
    attribute: String<U256>,
    value: String<U256>,
}

impl<'a> Request<'a> {
    pub fn restore_value(&mut self) {
        let mut new_value: String<U256> = String::new();
        for byte in self.value.as_str().chars() {
            if byte == '\'' {
                new_value.push('"').unwrap();
            } else {
                new_value.push(byte).unwrap();
            }
        }

        self.value = new_value;
    }
}

impl Response {
    /// Remove all double quotation marks from the `value` field of a response.
    fn sanitize_value(&mut self) {
        let mut new_value: String<U256> = String::new();
        for byte in self.value.as_str().chars() {
            if byte == '"' {
                new_value.push('\'').unwrap();
            } else {
                new_value.push(byte).unwrap();
            }
        }

        self.value = new_value;
    }

    /// Remove all double quotation marks from the `value` field of a response and wrap it in single
    /// quotes.
    fn wrap_and_sanitize_value(&mut self) {
        let mut new_value: String<U256> = String::new();
        new_value.push('\'').unwrap();
        for byte in self.value.as_str().chars() {
            if byte == '"' {
                new_value.push('\'').unwrap();
            } else {
                new_value.push(byte).unwrap();
            }
        }
        new_value.push('\'').unwrap();

        self.value = new_value;
    }

    /// Construct a successful reply.
    ///
    /// Note: `value` will be sanitized to convert all single quotes to double quotes.
    ///
    /// Args:
    /// * `attrbute` - The attribute of the success.
    /// * `value` - The value of the attribute.
    pub fn success(attribute: &str, value: &str) -> Self {
        let mut res = Self {
            code: 200,
            attribute: String::from(attribute),
            value: String::from(value),
        };
        res.sanitize_value();
        res
    }

    /// Construct an error reply.
    ///
    /// Note: `message` will be sanitized to convert all single quotes to double quotes.
    ///
    /// Args:
    /// * `attrbute` - The attribute of the success.
    /// * `message` - The message denoting the error.
    pub fn error(attribute: &str, message: &str) -> Self {
        let mut res = Self {
            code: 400,
            attribute: String::from(attribute),
            value: String::from(message),
        };
        res.wrap_and_sanitize_value();
        res
    }

    /// Construct a custom reply.
    ///
    /// Note: `message` will be sanitized to convert all single quotes to double quotes.
    ///
    /// Args:
    /// * `attrbute` - The attribute of the success.
    /// * `message` - The message denoting the status.
    pub fn custom(code: i32, message: &str) -> Self {
        let mut res = Self {
            code,
            attribute: String::from(""),
            value: String::from(message),
        };
        res.wrap_and_sanitize_value();
        res
    }
}

#[derive(Serialize)]
pub struct Status {
    pub t: u32,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

pub fn json_reply<T: Serialize>(socket: &mut net::socket::TcpSocket, msg: &T) {
    let mut u: String<U512> = to_string(msg).unwrap();
    u.push('\n').unwrap();
    socket.write_str(&u).unwrap();
}

pub struct Server {
    data: Vec<u8, U256>,
    discard: bool,
}

impl Server {
    /// Construct a new server object for managing requests.
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            discard: false,
        }
    }

    /// Poll the server for potential data updates.
    ///
    /// Args:
    /// * `socket` - The socket to check contents from.
    /// * `f` - A closure that can be called if a request has been received on the server.
    pub fn poll<F>(&mut self, socket: &mut net::socket::TcpSocket, mut f: F)
    where
        F: FnMut(&Request) -> Response,
    {
        while socket.can_recv() {
            let found = socket
                .recv(|buf| {
                    let (len, found) =
                        match buf.iter().position(|&c| c as char == '\n') {
                            Some(end) => (end + 1, true),
                            None => (buf.len(), false),
                        };
                    if self.data.len() + len >= self.data.capacity() {
                        self.discard = true;
                        self.data.clear();
                    } else if !self.discard && len > 0 {
                        self.data.extend_from_slice(&buf[..len]).unwrap();
                    }
                    (len, found)
                })
                .unwrap();

            if found {
                if self.discard {
                    self.discard = false;
                    json_reply(
                        socket,
                        &Response::custom(520, "command buffer overflow"),
                    );
                } else {
                    let r = from_slice::<Request>(
                        &self.data[..self.data.len() - 1],
                    );
                    match r {
                        Ok((mut res, _)) => {
                            // Note that serde_json_core doesn't escape quotations within a string.
                            // To account for this, we manually translate all single quotes to
                            // double quotes. This occurs because we doubly-serialize this field in
                            // some cases.
                            res.restore_value();
                            let response = f(&res);
                            json_reply(socket, &response);
                        }
                        Err(err) => {
                            warn!("parse error {:?}", err);
                            json_reply(
                                socket,
                                &Response::custom(550, "parse error"),
                            );
                        }
                    }
                }
                self.data.clear();
            }
        }
    }
}
