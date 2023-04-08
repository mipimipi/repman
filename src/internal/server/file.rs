use crate::internal::server;

#[derive(Clone)]
pub struct Server {}

impl Server {
    pub fn new() -> Server {
        Server {}
    }
}

impl server::Server for Server {}
