#[macro_use]
extern crate rocket;

pub mod commands;
pub mod config;
pub mod logsdb;
pub mod routes;
pub mod vrchat;

use std::fmt::Display;

use rocket::response::status::BadRequest;

pub fn bad_request<E: Display>(error: E) -> BadRequest<String> {
    eprintln!("Bad Request: {error}");
    BadRequest(error.to_string())
}

pub struct Data {
    pub config: config::Config,
    pub logsdb: logsdb::LogsDB,
    pub vrchat: vrchat::VRChat,
}
