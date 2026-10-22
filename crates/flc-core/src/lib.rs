//! FLC 核心：版本解析、下载、Java 管理、启动。不依赖任何 UI。
pub mod auth;
pub mod content;
pub mod download;
pub mod event;
pub mod instance;
pub mod java;
pub mod launch;
pub mod loader;
pub mod msa;
pub mod model;
pub mod rules;
pub mod skin;
pub mod vault;
pub mod version;

pub use auth::Account;
pub use download::{Downloader, Mirror};
pub use event::{Event, Sink};
pub use launch::{install, install_version, launch, Config, RunningGame};
