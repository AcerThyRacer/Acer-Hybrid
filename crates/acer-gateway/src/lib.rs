//! Local API gateway for Acer Hybrid

mod handlers;
mod openai_types;
mod server;

pub use openai_types::*;
pub use server::GatewayServer;
