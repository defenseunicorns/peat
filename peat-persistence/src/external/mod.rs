//! External API module for HTTP/REST access

pub mod routes;
pub mod server;

pub use server::{Server, ServerConfig};
