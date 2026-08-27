#[path = "support/mod.rs"]
mod support;

use support::generate;

const MEDIA_S3_TR: &str = r#"type MediaObject {
    key: String,
    content_type: String
}

infra MediaInfra {
    storage {
        s3 {
            bucket: "media"
            entity: MediaObject
        }
    }
}

use_case RequestUploadUrlUseCase {
    blob: MediaInfra.presigned_put_url
}

use_case RequestDownloadUrlUseCase {
    blob: MediaInfra.presigned_get_url
}

use_case DeleteMediaUseCase {
    blob: MediaInfra.delete
}

adapter HttpApi {
    http {
        on: "0.0.0.0:8080"
        routes {
            POST "/media/upload-url" {
                use_case: RequestUploadUrlUseCase
            }
            GET "/media/download-url" {
                use_case: RequestDownloadUrlUseCase
            }
            DELETE "/media" {
                use_case: DeleteMediaUseCase
            }
        }
    }
}

app Main: run {
    HttpApi
}"#;

#[test]
fn s3_infra_generates_a_cloneable_storage_struct_over_the_entity_name() {
    let project = generate(MEDIA_S3_TR);

    let storage = project.read("main/src/infrastructures/storage/s3/media_object.rs");
    assert!(storage.contains("pub struct S3MediaObjectStorage"));
    assert!(storage.contains("#[derive(Clone)]"));
    assert!(storage.contains("bucket: Box<Bucket>"));
    assert!(
        storage
            .contains("pub async fn presigned_put_url(&self, key: &str) -> Result<String, String>")
    );
    assert!(
        storage
            .contains("pub async fn presigned_get_url(&self, key: &str) -> Result<String, String>")
    );
    assert!(storage.contains("pub async fn delete(&self, key: &str) -> Result<(), String>"));
}

#[test]
fn blob_use_case_gets_a_fixed_key_in_url_out_shape() {
    let project = generate(MEDIA_S3_TR);

    let uc = project.read("main/src/applications/request_upload_url_use_case.rs");
    assert!(uc.contains("pub struct RequestUploadUrlUseCaseInput {\n    pub key: String,\n}"));
    assert!(uc.contains("pub struct RequestUploadUrlUseCaseOutput {\n    pub url: String,\n}"));
    assert!(uc.contains(
        "pub enum RequestUploadUrlUseCaseError {\n    NotFound,\n    StorageError(String),\n}"
    ));
    // Concrete field, not a generic `S0: Storage<T, E>` bound — blob storage
    // doesn't implement that trait.
    assert!(uc.contains("media_infra: S3MediaObjectStorage,"));
    assert!(uc.contains(
        "self.media_infra.presigned_put_url(&input.key).await.map_err(RequestUploadUrlUseCaseError::StorageError)?"
    ));
}

#[test]
fn blob_delete_echoes_the_key_back_as_output() {
    let project = generate(MEDIA_S3_TR);

    let uc = project.read("main/src/applications/delete_media_use_case.rs");
    assert!(uc.contains("pub struct DeleteMediaUseCaseOutput {\n    pub key: String,\n}"));
    assert!(uc.contains(
        "self.media_infra.delete(&input.key).await.map_err(DeleteMediaUseCaseError::StorageError)?"
    ));
    assert!(uc.contains("Ok(DeleteMediaUseCaseOutput { key: input.key })"));
}

#[test]
fn s3_infra_requires_the_rust_s3_crate_and_never_pulls_in_sqlx() {
    let project = generate(MEDIA_S3_TR);

    let cargo_toml = project.read("main/Cargo.toml");
    assert!(cargo_toml.contains("rust-s3 = \"0.37\""));
    // The blob-storage entity is metadata-only — it must never get
    // `sqlx::FromRow`/the `sqlx` dependency the way a psql entity does.
    assert!(!cargo_toml.contains("sqlx"));
    let domain = project.read("main/src/domains/media_object.rs");
    assert!(!domain.contains("sqlx::FromRow"));
}

#[test]
fn main_reads_three_env_vars_derived_from_the_infra_name_and_passes_the_literal_bucket() {
    let project = generate(MEDIA_S3_TR);

    let main = project.read("main/src/main.rs");
    assert!(main.contains(r#"std::env::var("MEDIA_INFRA_S3_ENDPOINT")"#));
    assert!(main.contains(r#"std::env::var("MEDIA_INFRA_S3_ACCESS_KEY")"#));
    assert!(main.contains(r#"std::env::var("MEDIA_INFRA_S3_SECRET_KEY")"#));
    assert!(main.contains(r#"S3MediaObjectStorage::new(media_infra_s3_endpoint, media_infra_s3_access_key, media_infra_s3_secret_key, "media")?"#));
}

#[test]
fn two_infras_sharing_one_entity_get_distinct_main_rs_variables() {
    // Regression: a psql metadata infra and an s3 blob infra targeting the
    // *same* entity (a real, intended pattern — see S3InfraService's doc
    // comment) used to both emit `let media_object_storage = ...;` in
    // main.rs, since the variable was named after the entity rather than
    // the infra — the second binding silently shadowed the first, so
    // whichever use case needed the other backend got the wrong type.
    let project = generate(
        r#"type MediaObject {
            id: Int,
            key: String
        }

        infra MediaMetaInfra {
            storage {
                psql {
                    entity: MediaObject
                }
            }
        }

        infra MediaBlobInfra {
            storage {
                s3 {
                    bucket: "media"
                    entity: MediaObject
                }
            }
        }

        use_case ConfirmUploadUseCase {
            call: MediaMetaInfra.save_one
        }

        use_case RequestUploadUrlUseCase {
            blob: MediaBlobInfra.presigned_put_url
        }

        adapter HttpApi {
            http {
                on: "0.0.0.0:8080"
                routes {
                    POST "/media" {
                        use_case: ConfirmUploadUseCase
                    }
                    POST "/media/upload-url" {
                        use_case: RequestUploadUrlUseCase
                    }
                }
            }
        }

        app Main: run {
            HttpApi
        }"#,
    );

    let main = project.read("main/src/main.rs");
    assert!(main.contains("let media_meta_infra_storage = PsqlMediaObjectStorage::new"));
    assert!(main.contains("let media_blob_infra_storage = S3MediaObjectStorage::new"));
    assert!(main.contains("ConfirmUploadUseCase::new(media_meta_infra_storage.clone())"));
    assert!(main.contains("RequestUploadUrlUseCase::new(media_blob_infra_storage.clone())"));
}
