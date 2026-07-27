pub mod _trait;
pub mod cargo_toml;
pub mod dev_docs;
pub mod env_template;
pub mod imports;
pub mod include;
pub mod proto;
pub mod render;
pub mod service;
pub mod types;
pub mod file;
pub mod _enum;

pub trait RustCode {
    fn to_rust_code(&self) -> String;
}
