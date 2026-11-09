use std::{fs::File, io::Write, path::PathBuf};

use heck::ToSnakeCase;

use crate::generator::rust_code::{imports::{common_use, use_statement}, render::render};
use tera::Context;

/// The three operations an `s3 {}` infra supports today — validated here
/// (codegen time), not in the parser, mirroring how `grpc_client`'s
/// `methods {}` storage-method names are validated at codegen rather than
/// parse time.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BlobOperation {
    PresignedPutUrl,
    PresignedGetUrl,
    Delete,
}

impl BlobOperation {
    pub fn from_operation(operation: &str) -> Result<Self, String> {
        match operation {
            "presigned_put_url" => Ok(BlobOperation::PresignedPutUrl),
            "presigned_get_url" => Ok(BlobOperation::PresignedGetUrl),
            "delete" => Ok(BlobOperation::Delete),
            other => Err(format!(
                "Unsupported blob operation '{}': expected one of presigned_put_url, presigned_get_url, delete",
                other
            )),
        }
    }

    /// Every blob use case takes the same fixed `{ key: String }` input —
    /// there's no entity-driven guessing here (unlike CRUD's `call:`),
    /// since a blob operation always addresses one object by its key.
    /// `pub` so `register_use_case_io` (the `--dev` docs pre-pass) can
    /// reuse the exact same shape rather than duplicating it.
    pub fn input_fields() -> Vec<(&'static str, &'static str)> {
        vec![("key", "String")]
    }

    /// `presigned_put_url`/`presigned_get_url` return the generated URL;
    /// `delete` echoes the key back, confirming which object was removed.
    pub fn output_fields(&self) -> Vec<(&'static str, &'static str)> {
        match self {
            BlobOperation::PresignedPutUrl | BlobOperation::PresignedGetUrl => {
                vec![("url", "String")]
            }
            BlobOperation::Delete => vec![("key", "String")],
        }
    }

    fn storage_method(&self) -> &'static str {
        match self {
            BlobOperation::PresignedPutUrl => "presigned_put_url",
            BlobOperation::PresignedGetUrl => "presigned_get_url",
            BlobOperation::Delete => "delete",
        }
    }

    fn body(&self, field: &str, error: &str, output: &str) -> String {
        let mut context = tera::Context::new();
        context.insert("field", field);
        context.insert("method", self.storage_method());
        context.insert("error", error);
        context.insert("output", output);
        crate::generator::rust_code::render::render("file/blob_use_case/body.rs.tera", &context)
    }
}

/// Generates a use case bound to one `s3 {}` infra operation via `blob:`.
/// Unlike a CRUD use case, the field holding the infra is a concrete type
/// (`S3{Entity}Storage`), not a generic `S0: Storage<T, E>` bound — blob
/// storage doesn't implement that shared trait (see `S3InfraService`'s doc
/// comment), so there's no backend to be generic over yet. The HTTP
/// adapter's own genericity (`U0: {Name}Trait + Send + Sync + 'static`)
/// still applies regardless — see `{Name}Trait` below — so this is fully
/// compatible with the existing adapter codegen despite not being generic
/// itself.
pub struct BlobUseCaseFile {
    name: String,
    operation: BlobOperation,
    infra_field: String,
    storage_type: String,
    file: File,
}

impl BlobUseCaseFile {
    pub fn new(
        name: String,
        operation: BlobOperation,
        infra_field: String,
        storage_type: String,
        folder: PathBuf,
    ) -> Result<Self, String> {
        let file_path = folder.join(format!("{}.rs", name.to_snake_case()));

        Ok(BlobUseCaseFile {
            file: File::create(&file_path)
                .map_err(|e| format!("Failed to create use case file: {}", e))?,
            name,
            operation,
            infra_field,
            storage_type,
        })
    }

    fn input_name(&self) -> String {
        format!("{}Input", self.name)
    }

    fn output_name(&self) -> String {
        format!("{}Output", self.name)
    }

    fn error_name(&self) -> String {
        format!("{}Error", self.name)
    }

    fn trait_name(&self) -> String {
        format!("{}Trait", self.name)
    }

    fn gen_imports(&self) -> String {
        let mut code = common_use(&["UseCase"]);
        code.push_str(&use_statement("crate", &[&self.storage_type]));
        code.push('\n');
        code
    }

    fn gen_input_struct(&self) -> String {
        #[derive(serde::Serialize)]
        struct FieldView {
            name: String,
            field_type: String,
        }
        let fields: Vec<FieldView> = BlobOperation::input_fields()
            .into_iter()
            .map(|(name, ty)| FieldView {
                name: name.to_string(),
                field_type: ty.to_string(),
            })
            .collect();

        let mut context = Context::new();
        context.insert("name", &self.input_name());
        context.insert("fields", &fields);
        render("file/use_case/input_struct.rs.tera", &context)
    }

    fn gen_output_struct(&self) -> String {
        #[derive(serde::Serialize)]
        struct FieldView {
            name: String,
            field_type: String,
        }
        let fields: Vec<FieldView> = self
            .operation
            .output_fields()
            .into_iter()
            .map(|(name, ty)| FieldView {
                name: name.to_string(),
                field_type: ty.to_string(),
            })
            .collect();

        let mut context = Context::new();
        context.insert("name", &self.output_name());
        context.insert("fields", &fields);
        render("file/blob_use_case/output_struct.rs.tera", &context)
    }

    /// Fixed at exactly `NotFound`/`StorageError(String)` — same precedent
    /// `ai {}` use cases already set (see `file/ai_use_case/error.rs.tera`,
    /// reused verbatim here): `NotFound` is never actually constructed by a
    /// blob operation today, but it stays part of the shape so the adapter's
    /// status-mapping match (which always emits `NotFound -> 404`
    /// unconditionally) stays exhaustive regardless of use case kind.
    fn gen_error(&self) -> String {
        let mut context = Context::new();
        context.insert("name", &self.error_name());
        context.insert("storage_error", &None::<String>);
        render("file/ai_use_case/error.rs.tera", &context)
    }

    fn gen_trait(&self) -> String {
        let mut context = Context::new();
        context.insert("trait_name", &self.trait_name());
        context.insert("input", &self.input_name());
        context.insert("output", &self.output_name());
        context.insert("error", &self.error_name());
        render("file/ai_use_case/trait.rs.tera", &context)
    }

    fn gen_struct(&self) -> String {
        let mut context = Context::new();
        context.insert("name", &self.name);
        context.insert("field", &self.infra_field);
        context.insert("storage_type", &self.storage_type);
        render("file/blob_use_case/struct.rs.tera", &context)
    }

    fn gen_impl(&self) -> String {
        let error = self.error_name();
        let output = self.output_name();
        let body = self.operation.body(&self.infra_field, &error, &output);

        let mut context = Context::new();
        context.insert("input", &self.input_name());
        context.insert("output", &output);
        context.insert("error", &error);
        context.insert("name", &self.name);
        context.insert("trait_name", &self.trait_name());
        context.insert("body", &body);
        render("file/blob_use_case/impl.rs.tera", &context)
    }

    pub fn write(&mut self) -> Result<(), String> {
        let mut code = String::new();
        code.push_str(&self.gen_imports());
        code.push_str(&self.gen_input_struct());
        code.push_str(&self.gen_output_struct());
        code.push_str(&self.gen_error());
        code.push_str(&self.gen_trait());
        code.push_str(&self.gen_struct());
        code.push_str(&self.gen_impl());
        self.file
            .write_all(code.as_bytes())
            .map_err(|e| format!("Failed to write to use case file: {}", e))
    }
}
