//! Local API gateway for Acer Hybrid

mod server;
mod handlers;
mod openai_types;

pub use server::GatewayServer;
pub use openai_types::*;