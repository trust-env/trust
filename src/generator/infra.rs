mod storage;

use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

use crate::{
    generator::{CodeGen, CodeGenContext, rust_code::{RustCode, service::Service}}, parser::{Ast, InfraService, InfraServiceType::Storage, PsqlInfraService, StorageType::Psql},
};

struct CodeInfraContext<'a> {
    folder: &'a PathBuf,
    service: &'a Service,
}

impl CodeGen {
    pub(super) fn generate_infra(
        &mut self,
        context: &mut CodeGenContext,
        infra: InfraService,
        folder: PathBuf,
        full_asts: &[Ast],
        proto_dir: &PathBuf,
    ) -> Result<(), String> {
        let service = Service::new(infra.name);

        match infra.service_type {
            Storage(s) => self.gen_storage(context, s, folder, full_asts, proto_dir),
            _ => Err("Unknown combination of keywords".to_string()),
        }
    }
}
