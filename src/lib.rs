pub mod commands;
pub mod config;
pub mod logsdb;
pub mod vrchat;

pub struct Data {
    pub config: config::Config,
    pub logsdb: logsdb::LogsDB,
    pub vrchat: vrchat::VRChat,
}
