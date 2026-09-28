use crate::{
    lexer::{Keyword, Token},
    parser::{
        Ast, GrpcClientInfraService, GrpcClientMethodMapping, HttpClientInfraService, InfraService,
        InfraServiceType, InfraServiceType::Storage, Parser, PsqlInfraService, S3InfraService,
        StorageType, StorageType::Psql,
    },
};

impl Parser {
    fn unwrap_infra(&mut self) -> Result<Ast, String> {
        let mut keywords: Vec<Keyword> = Vec::new();
        let mut i = 0;
        let name;

        loop {
            match self.next_token() {
                Some(Token::Identifier(n)) => {
                    name = n.clone();
                    break;
                }
                Some(Token::Keyword(k)) => {
                    i += 1;
                    keywords.push(k);
                }
                Some(Token::Colon) => i += 1,
                _ => {
                    self.revert(i);
                    return Err("Unexpected token while parsing InfraService".to_string());
                }
            }
        }
        match keywords.as_slice() {
            [Keyword::Storage, Keyword::Psql] => Ok(Ast::Infra(InfraService {
                name,
                service_type: Storage(Psql(self.infra_psql_body()?)),
            })),
            _ => return Err("Unknown combination of keywords".to_string()),
        }
    }

    fn infra_psql_body(&mut self) -> Result<PsqlInfraService, String> {
        match self.next_token() {
            Some(Token::OpenBrace) => {}
            _ => {
                self.revert(1);
                return Err("Expected '{' after Psql declaration".to_string());
            }
        }

        let entity = match (self.next_token(), self.next_token()) {
            (Some(Token::Keyword(Keyword::Entity)), Some(Token::Colon)) => self.try_type()?,
            _ => {
                self.revert(2);
                return Err("Expected 'entity' after Psql declaration".to_string());
            }
        };

        let mut seed = None;
        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => return Ok(PsqlInfraService { entity, seed }),
                Some(Token::Comma) => continue,
                Some(Token::Identifier(k)) if k == "seed" => {
                    match (self.next_token(), self.next_token()) {
                        (Some(Token::Colon), Some(Token::StringLiteral(path))) => seed = Some(path),
                        _ => return Err("Expected ': \"<path>\"' after 'seed'".to_string()),
                    }
                }
                _ => {
                    self.revert(2);
                    return Err("Expected '}' after Psql declaration".to_string());
                }
            }
        }
    }

    /// Parses `grpc_client { service: X on: "..." entity: Y methods {
    /// <storage_method>: <RpcName> ... } }`. Each `methods` line is a bare
    /// identifier pair (not fixed keywords) — validated against the known
    /// `Storage<T, E>` verbs at generation time, not here, mirroring how
    /// `errors {}` source validation happens in `use_case.rs` rather than
    /// the parser.
    fn infra_grpc_client_body(&mut self) -> Result<GrpcClientInfraService, String> {
        match self.next_token() {
            Some(Token::OpenBrace) => {}
            _ => {
                self.revert(1);
                return Err("Expected '{' after GrpcClient declaration".to_string());
            }
        }

        let service = match (self.next_token(), self.next_token()) {
            (Some(Token::Keyword(Keyword::Service)), Some(Token::Colon)) => match self.next_token() {
                Some(Token::Identifier(name)) => name,
                _ => {
                    self.revert(3);
                    return Err("Expected identifier after 'service:'".to_string());
                }
            },
            _ => {
                self.revert(2);
                return Err("Expected 'service' after GrpcClient declaration".to_string());
            }
        };

        let on = match (self.next_token(), self.next_token()) {
            (Some(Token::Keyword(Keyword::On)), Some(Token::Colon)) => match self.next_token() {
                Some(Token::StringLiteral(address)) => address,
                _ => {
                    self.revert(3);
                    return Err("Expected string literal after 'on:'".to_string());
                }
            },
            _ => {
                self.revert(2);
                return Err("Expected 'on' after 'service' in GrpcClient declaration".to_string());
            }
        };

        let entity = match (self.next_token(), self.next_token()) {
            (Some(Token::Keyword(Keyword::Entity)), Some(Token::Colon)) => self.try_type()?,
            _ => {
                self.revert(2);
                return Err("Expected 'entity' after 'on' in GrpcClient declaration".to_string());
            }
        };

        let methods = match (self.next_token(), self.next_token()) {
            (Some(Token::Keyword(Keyword::Methods)), Some(Token::OpenBrace)) => {
                let mut methods = Vec::new();
                loop {
                    match self.next_token() {
                        Some(Token::CloseBrace) => break,
                        Some(Token::Identifier(storage_method)) => {
                            match (self.next_token(), self.next_token()) {
                                (Some(Token::Colon), Some(Token::Identifier(rpc))) => {
                                    methods.push(GrpcClientMethodMapping { storage_method, rpc });
                                }
                                _ => {
                                    self.revert(3);
                                    return Err(
                                        "Expected '<storage_method>: <RpcName>' in 'methods' block"
                                            .to_string(),
                                    );
                                }
                            }
                        }
                        _ => {
                            self.revert(1);
                            return Err(
                                "Expected '<storage_method>: <RpcName>' or '}' in 'methods' block"
                                    .to_string(),
                            );
                        }
                    }
                }
                methods
            }
            _ => {
                self.revert(2);
                return Err("Expected 'methods' block after 'entity' in GrpcClient declaration".to_string());
            }
        };

        match self.next_token() {
            Some(Token::CloseBrace) => Ok(GrpcClientInfraService { service, on, entity, methods }),
            _ => {
                self.revert(1);
                return Err("Expected '}' after GrpcClient declaration".to_string());
            }
        }
    }

    /// Parses `s3 { bucket: "..." entity: MediaObject }`. `bucket` is a
    /// literal string (not secret, so it's fine in source — unlike
    /// endpoint/access key/secret key, which are never declared in `.tr` at
    /// all and are always read from env vars at runtime, same precedent as
    /// psql's connection string). No shorthand form, same as `grpc_client`.
    fn infra_s3_body(&mut self) -> Result<S3InfraService, String> {
        match self.next_token() {
            Some(Token::OpenBrace) => {}
            _ => {
                self.revert(1);
                return Err("Expected '{' after S3 declaration".to_string());
            }
        }

        let bucket = match (self.next_token(), self.next_token()) {
            (Some(Token::Keyword(Keyword::Bucket)), Some(Token::Colon)) => {
                match self.next_token() {
                    Some(Token::StringLiteral(bucket)) => bucket,
                    _ => {
                        self.revert(3);
                        return Err("Expected string literal after 'bucket:'".to_string());
                    }
                }
            }
            _ => {
                self.revert(2);
                return Err("Expected 'bucket' after S3 declaration".to_string());
            }
        };

        let entity = match (self.next_token(), self.next_token()) {
            (Some(Token::Keyword(Keyword::Entity)), Some(Token::Colon)) => self.try_type()?,
            _ => {
                self.revert(2);
                return Err("Expected 'entity' after 'bucket' in S3 declaration".to_string());
            }
        };

        match self.next_token() {
            Some(Token::CloseBrace) => Ok(S3InfraService { bucket, entity }),
            _ => {
                self.revert(1);
                return Err("Expected '}' after S3 declaration".to_string());
            }
        }
    }

    fn infra_storage_body(&mut self) -> Result<StorageType, String> {
        match self.next_token() {
            Some(Token::OpenBrace) => {}
            _ => {
                self.revert(1);
                return Err("Expected '{' after Storage declaration".to_string());
            }
        }

        let res = match self.next_token() {
            Some(Token::Keyword(Keyword::Psql)) => Psql(self.infra_psql_body()?),
            Some(Token::Keyword(Keyword::GrpcClient)) => {
                StorageType::GrpcClient(self.infra_grpc_client_body()?)
            }
            Some(Token::Keyword(Keyword::S3)) => StorageType::S3(self.infra_s3_body()?),
            _ => {
                self.revert(2);
                return Err("Expected 'psql', 'grpc_client', or 's3' after Storage declaration".to_string());
            }
        };

        match self.next_token() {
            Some(Token::CloseBrace) => Ok(res),
            _ => {
                self.revert(2);
                return Err("Expected '}' after Storage declaration".to_string());
            }
        }
    }

    /// Parses `{ base_url: "..." headers { "Name": "value" ... } }` after
    /// `http_client`. Header names are quoted (they contain `-`).
    fn infra_http_client_body(&mut self) -> Result<HttpClientInfraService, String> {
        if self.next_token() != Some(Token::OpenBrace) {
            return Err("Expected '{' after 'http_client'".to_string());
        }
        let mut base_url = None;
        let mut headers = Vec::new();
        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => break,
                Some(Token::Comma) => continue,
                Some(Token::Identifier(k)) if k == "base_url" => {
                    match (self.next_token(), self.next_token()) {
                        (Some(Token::Colon), Some(Token::StringLiteral(url))) => {
                            base_url = Some(url)
                        }
                        _ => return Err("Expected ': \"<url>\"' after 'base_url'".to_string()),
                    }
                }
                Some(Token::Identifier(k)) if k == "headers" => {
                    if self.next_token() != Some(Token::OpenBrace) {
                        return Err("Expected '{' after 'headers'".to_string());
                    }
                    loop {
                        match self.next_token() {
                            Some(Token::CloseBrace) => break,
                            Some(Token::Comma) => continue,
                            Some(Token::StringLiteral(name)) => {
                                match (self.next_token(), self.next_token()) {
                                    (Some(Token::Colon), Some(Token::StringLiteral(value))) => {
                                        headers.push((name, value))
                                    }
                                    _ => {
                                        return Err(format!(
                                            "Expected ': \"<value>\"' after header '{}'",
                                            name
                                        ));
                                    }
                                }
                            }
                            _ => return Err(
                                "Expected '\"<Header-Name>\": \"<value>\"' entries in 'headers'"
                                    .to_string(),
                            ),
                        }
                    }
                }
                _ => return Err("Unexpected token in 'http_client' block".to_string()),
            }
        }
        let base_url = base_url.ok_or("'http_client' block must declare 'base_url'")?;
        Ok(HttpClientInfraService { base_url, headers })
    }

    fn infra_content(&mut self) -> Result<InfraServiceType, String> {
        let mut i = 0;
        let mut res: Option<InfraServiceType> = None;

        match self.next_token() {
            Some(Token::OpenBrace) => {}
            _ => {
                self.revert(1);
                return Err("Expected '{' after InfraService declaration".to_string());
            }
        }
        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => break,
                Some(Token::Keyword(Keyword::Storage)) => {
                    res = Some(Storage(self.infra_storage_body()?))
                }
                Some(Token::Identifier(k)) if k == "http_client" => {
                    res = Some(InfraServiceType::HttpClient(self.infra_http_client_body()?))
                }
                _ => {
                    self.revert(i + 1);
                    return Err("Unexpected token in InfraService body".to_string());
                }
            }
            i += 1;
        }
        match res {
            Some(service_type) => Ok(service_type),
            None => Err("No valid service type found in InfraService body".to_string()),
        }
    }

    pub(super) fn try_infra_service(&mut self) -> Result<Ast, String> {
        match self.next_token() {
            Some(Token::Keyword(Keyword::Infra)) => match self.next_token() {
                Some(Token::Colon) => self.unwrap_infra(),
                Some(Token::Identifier(name)) => {
                    let service_type = self.infra_content()?;
                    Ok(Ast::Infra(InfraService { name, service_type }))
                }
                _ => {
                    self.revert(2);
                    return Err("Failed to parse InfraService".to_string());
                }
            },
            _ => {
                self.revert(1);
                return Err("Failed to parse InfraService".to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer::Lexer, parser::AllowedType};

    fn parser_for(src: &str) -> Parser {
        let tokens = Lexer::new(&src.to_string()).run().expect("lex should succeed");
        Parser::new(tokens)
    }

    fn parse_infra(src: &str) -> InfraService {
        match parser_for(src).try_infra_service().unwrap() {
            Ast::Infra(i) => i,
            other => panic!("expected Ast::Infra, got {:?}", other),
        }
    }

    fn entity_of(infra: &InfraService) -> &AllowedType {
        match &infra.service_type {
            Storage(Psql(psql)) => &psql.entity,
            Storage(StorageType::GrpcClient(cfg)) => &cfg.entity,
            Storage(StorageType::S3(cfg)) => &cfg.entity,
            InfraServiceType::HttpClient(_) => panic!("http_client infras have no entity"),
        }
    }

    #[test]
    fn http_client_block_parses_base_url_and_headers() {
        let infra = parse_infra(
            r#"infra PaymentApi {
                http_client {
                    base_url: "${PAYMENT_API_URL}"
                    headers {
                        "X-API-Key": "${PAYMENT_API_KEY}"
                        "Accept": "application/json"
                    }
                }
            }"#,
        );
        match infra.service_type {
            InfraServiceType::HttpClient(cfg) => {
                assert_eq!(cfg.base_url, "${PAYMENT_API_URL}");
                assert_eq!(
                    cfg.headers,
                    vec![
                        ("X-API-Key".to_string(), "${PAYMENT_API_KEY}".to_string()),
                        ("Accept".to_string(), "application/json".to_string()),
                    ]
                );
            }
            other => panic!("expected an http_client infra, got {:?}", other),
        }
    }

    #[test]
    fn http_client_without_base_url_is_rejected() {
        let err = parser_for(r#"infra PaymentApi { http_client { headers { "A": "b" } } }"#)
            .try_infra_service()
            .unwrap_err();
        assert!(err.contains("base_url"), "{err}");
    }

    #[test]
    fn block_form_and_shorthand_form_produce_the_same_infra_service() {
        let block_form = parse_infra(
            r#"infra Users {
                storage {
                    psql {
                        entity: User
                    }
                }
            }"#,
        );
        let shorthand_form = parse_infra("infra: storage psql Users { entity: User }");

        assert_eq!(block_form.name, "Users");
        assert_eq!(shorthand_form.name, "Users");
        assert_eq!(entity_of(&block_form), &AllowedType::DefinedType("User".to_string()));
        assert_eq!(entity_of(&shorthand_form), &AllowedType::DefinedType("User".to_string()));
    }

    #[test]
    fn shorthand_form_rejects_unknown_keyword_combination() {
        // Only `storage psql` is a recognized combination today.
        let err = parser_for("infra: psql storage Users { entity: User }")
            .try_infra_service()
            .unwrap_err();
        assert_eq!(err, "Unknown combination of keywords");
    }

    #[test]
    fn block_form_requires_a_storage_keyword_inside_the_body() {
        let src = "infra Users { psql { entity: User } }";
        assert!(parser_for(src).try_infra_service().is_err());
    }

    #[test]
    fn missing_infra_keyword_is_an_error() {
        let err = parser_for("not_infra Users {}").try_infra_service().unwrap_err();
        assert_eq!(err, "Failed to parse InfraService");
    }

    #[test]
    fn grpc_client_block_parses_all_fields_and_method_mappings() {
        let infra = parse_infra(
            r#"infra RemoteTaskInfra {
                storage {
                    grpc_client {
                        service: GrpcWorkerAdapter
                        on: "127.0.0.1:50051"
                        entity: Task
                        methods {
                            find_one: GetTask
                        }
                    }
                }
            }"#,
        );

        assert_eq!(infra.name, "RemoteTaskInfra");
        match &infra.service_type {
            Storage(StorageType::GrpcClient(cfg)) => {
                assert_eq!(cfg.service, "GrpcWorkerAdapter");
                assert_eq!(cfg.on, "127.0.0.1:50051");
                assert_eq!(cfg.entity, AllowedType::DefinedType("Task".to_string()));
                assert_eq!(cfg.methods.len(), 1);
                assert_eq!(cfg.methods[0].storage_method, "find_one");
                assert_eq!(cfg.methods[0].rpc, "GetTask");
            }
            other => panic!("expected Storage(GrpcClient(_)), got {:?}", other),
        }
    }

    #[test]
    fn grpc_client_methods_block_can_declare_more_than_one_mapping() {
        let infra = parse_infra(
            r#"infra RemoteTaskInfra {
                storage {
                    grpc_client {
                        service: GrpcWorkerAdapter
                        on: "127.0.0.1:50051"
                        entity: Task
                        methods {
                            find_one: GetTask
                            save_one: CreateTask
                        }
                    }
                }
            }"#,
        );

        match &infra.service_type {
            Storage(StorageType::GrpcClient(cfg)) => {
                assert_eq!(cfg.methods.len(), 2);
                assert_eq!(cfg.methods[1].storage_method, "save_one");
                assert_eq!(cfg.methods[1].rpc, "CreateTask");
            }
            other => panic!("expected Storage(GrpcClient(_)), got {:?}", other),
        }
    }

    #[test]
    fn s3_block_parses_bucket_and_entity() {
        let infra = parse_infra(
            r#"infra MediaInfra {
                storage {
                    s3 {
                        bucket: "media"
                        entity: MediaObject
                    }
                }
            }"#,
        );

        assert_eq!(infra.name, "MediaInfra");
        match &infra.service_type {
            Storage(StorageType::S3(cfg)) => {
                assert_eq!(cfg.bucket, "media");
                assert_eq!(cfg.entity, AllowedType::DefinedType("MediaObject".to_string()));
            }
            other => panic!("expected Storage(S3(_)), got {:?}", other),
        }
    }

    #[test]
    fn s3_requires_a_bucket_field() {
        let src = r#"infra MediaInfra {
            storage {
                s3 {
                    entity: MediaObject
                }
            }
        }"#;
        assert!(parser_for(src).try_infra_service().is_err());
    }

    #[test]
    fn s3_requires_an_entity_field() {
        let src = r#"infra MediaInfra {
            storage {
                s3 {
                    bucket: "media"
                }
            }
        }"#;
        assert!(parser_for(src).try_infra_service().is_err());
    }

    #[test]
    fn grpc_client_requires_a_methods_block() {
        let src = r#"infra RemoteTaskInfra {
            storage {
                grpc_client {
                    service: GrpcWorkerAdapter
                    on: "127.0.0.1:50051"
                    entity: Task
                }
            }
        }"#;
        assert!(parser_for(src).try_infra_service().is_err());
    }
}
