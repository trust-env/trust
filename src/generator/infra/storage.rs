mod grpc_client;
mod psql;
mod s3;

use std::{fs::create_dir_all, path::PathBuf};

use crate::{
    generator::{CodeGen, CodeGenContext}, parser::{Ast, InfraService, StorageType},
};

impl CodeGen {
    pub(super) fn gen_storage(
        &mut self,
        context: &mut CodeGenContext,
        storage: StorageType,
        folder: PathBuf,
        full_asts: &[Ast],
        proto_dir: &PathBuf,
    ) -> Result<(), String> {
        let storage_folder_path = folder.join("storage");

        // Shared across every infra, so it may already exist by the time a
        // second (or third) infra is generated.
        create_dir_all(&storage_folder_path)
            .map_err(|e| format!("Failed to create storage folder: {}", e))?;
        match storage {
            StorageType::Psql(psql) => self.gen_psql_storage(context, psql, storage_folder_path),
            StorageType::GrpcClient(cfg) => {
                self.gen_grpc_client_storage(context, cfg, storage_folder_path, full_asts, proto_dir)
            }
            StorageType::S3(cfg) => self.gen_s3_storage(context, cfg, storage_folder_path),
        }
    }
}
