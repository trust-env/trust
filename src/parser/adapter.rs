use crate::{
    lexer::{Keyword, Token},
    parser::{
        AdapterService, AdapterType, AllowedType, Ast, AuthRequirement, EventsEndpoint,
        GrpcAdapterService, GrpcRpc, HttpAdapterService, HttpMethod, HttpRoute, McpAdapterService,
        McpTool, Parser, PermissionRequirement, SecretRequirement, StatusMapping,
    },
};

impl Parser {
    fn try_http_method(&mut self) -> Result<HttpMethod, String> {
        match self.next_token() {
            Some(Token::Identifier(m)) => match m.as_str() {
                "GET" => Ok(HttpMethod::Get),
                "POST" => Ok(HttpMethod::Post),
                "PUT" => Ok(HttpMethod::Put),
                "PATCH" => Ok(HttpMethod::Patch),
                "DELETE" => Ok(HttpMethod::Delete),
                _ => {
                    self.revert(1);
                    Err(format!("Unknown HTTP method: {}", m))
                }
            },
            _ => {
                self.revert(1);
                Err("Expected HTTP method".to_string())
            }
        }
    }

    fn try_status_mapping(&mut self) -> Result<StatusMapping, String> {
        let target = match self.next_token() {
            Some(Token::Identifier(t)) => t,
            _ => {
                self.revert(1);
                return Err("Expected error identifier in 'status' block".to_string());
            }
        };
        match self.next_token() {
            Some(Token::Arrow) => {}
            _ => {
                self.revert(2);
                return Err("Expected '->' after error identifier in 'status' block".to_string());
            }
        }
        let status = match self.next_token() {
            Some(Token::Identifier(word)) if word.parse::<u16>().is_ok() => {
                word.parse::<u16>().unwrap()
            }
            _ => {
                self.revert(3);
                return Err("Expected numeric status code after '->'".to_string());
            }
        };
        Ok(StatusMapping { target, status })
    }

    fn try_status_block(&mut self) -> Result<Vec<StatusMapping>, String> {
        match (self.next_token(), self.next_token()) {
            (Some(Token::Keyword(Keyword::Status)), Some(Token::OpenBrace)) => {}
            _ => {
                self.revert(2);
                return Err("Expected 'status {' block".to_string());
            }
        }

        let mut mappings = Vec::new();
        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => break,
                _ => {
                    self.revert(1);
                    mappings.push(self.try_status_mapping()?);
                }
            }
        }
        Ok(mappings)
    }

    /// Parses the block form of `input`: `input { body: Type }`. Currently
    /// equivalent to `input: Type` — `body` just names the source
    /// explicitly, since the actual path-vs-body split is derived from
    /// `:name` segments in the route path.
    fn try_input_block(&mut self) -> Result<AllowedType, String> {
        let pos = self.current;
        match (self.next_token(), self.next_token()) {
            (Some(Token::Keyword(Keyword::Body)), Some(Token::Colon)) => {
                let t = self.try_type()?;
                match self.next_token() {
                    Some(Token::CloseBrace) => Ok(t),
                    _ => {
                        self.reset(pos);
                        Err("Expected '}' to close 'input' block".to_string())
                    }
                }
            }
            _ => {
                self.reset(pos);
                Err("Expected 'body:' inside 'input' block".to_string())
            }
        }
    }

    /// Parses `auth { infra: InfraName target: "..." action: "..." }`.
    /// `target`/`action` are quoted strings (matched against arbitrary
    /// stored permission values, not Rust identifiers), and `infra` reuses
    /// the existing `Keyword::Infra` token. Neither `target` nor `action`
    /// are reserved keywords — they're extremely common struct field names
    /// (e.g. on `Permission` itself), so they're matched as plain
    /// identifiers here instead.
    ///
    /// Also accepts the login-only shorthand `auth: required`, which needs a
    /// valid JWT but no permission row.
    fn try_auth_block(&mut self) -> Result<AuthRequirement, String> {
        match (self.next_token(), self.next_token()) {
            (Some(Token::Keyword(Keyword::Auth)), Some(Token::OpenBrace)) => {}
            (Some(Token::Keyword(Keyword::Auth)), Some(Token::Colon)) => {
                return match self.next_token() {
                    Some(Token::Identifier(s)) if s == "required" => {
                        Ok(AuthRequirement::Authenticated)
                    }
                    _ => {
                        self.revert(3);
                        Err("Expected 'required' after 'auth:'".to_string())
                    }
                };
            }
            _ => {
                self.revert(2);
                return Err("Expected 'auth {' block or 'auth: required'".to_string());
            }
        }

        let pos = self.current;
        let mut infra = None;
        let mut target = None;
        let mut action = None;
        let mut secret = None;
        let mut header = None;
        let mut user = 0;

        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => break,
                Some(Token::Comma) => continue,
                Some(Token::Keyword(Keyword::Infra)) => match self.next_token() {
                    Some(Token::Colon) => match self.next_token() {
                        Some(Token::Identifier(name)) => infra = Some(name),
                        _ => {
                            self.reset(pos);
                            return Err("Expected identifier after 'infra:'".to_string());
                        }
                    },
                    _ => {
                        self.reset(pos);
                        return Err("Expected ':' after 'infra'".to_string());
                    }
                },
                Some(Token::Identifier(s)) if s == "target" => match self.next_token() {
                    Some(Token::Colon) => match self.next_token() {
                        Some(Token::StringLiteral(v)) => target = Some(v),
                        _ => {
                            self.reset(pos);
                            return Err("Expected string literal after 'target:'".to_string());
                        }
                    },
                    _ => {
                        self.reset(pos);
                        return Err("Expected ':' after 'target'".to_string());
                    }
                },
                Some(Token::Identifier(s)) if s == "action" => match self.next_token() {
                    Some(Token::Colon) => match self.next_token() {
                        Some(Token::StringLiteral(v)) => action = Some(v),
                        _ => {
                            self.reset(pos);
                            return Err("Expected string literal after 'action:'".to_string());
                        }
                    },
                    _ => {
                        self.reset(pos);
                        return Err("Expected ':' after 'action'".to_string());
                    }
                },
                Some(Token::Identifier(s)) if s == "secret" || s == "header" => {
                    match (self.next_token(), self.next_token()) {
                        (Some(Token::Colon), Some(Token::StringLiteral(v))) => {
                            if s == "secret" { secret = Some(v) } else { header = Some(v) }
                        }
                        _ => {
                            self.reset(pos);
                            return Err(format!("Expected ': \"...\"' after '{}'", s));
                        }
                    }
                }
                Some(Token::Identifier(s)) if s == "user" => match (self.next_token(), self.next_token()) {
                    (Some(Token::Colon), Some(Token::Identifier(id))) if id.parse::<i64>().is_ok() => {
                        user = id.parse().unwrap_or(0)
                    }
                    _ => {
                        self.reset(pos);
                        return Err("Expected ': <user id>' after 'user'".to_string());
                    }
                },
                _ => {
                    self.reset(pos);
                    return Err("Failed to parse 'auth' block".to_string());
                }
            }
        }

        if let Some(secret) = secret {
            if infra.is_some() || target.is_some() || action.is_some() {
                self.reset(pos);
                return Err("'auth' uses either 'secret' or 'infra'/'target'/'action', not both".to_string());
            }
            return Ok(AuthRequirement::Secret(SecretRequirement { secret, header, user }));
        }

        match (infra, target, action) {
            (Some(infra), Some(target), Some(action)) => {
                Ok(AuthRequirement::Permission(PermissionRequirement {
                    infra,
                    target,
                    action,
                }))
            }
            _ => {
                self.reset(pos);
                Err("'auth' block must declare 'infra', 'target', and 'action'".to_string())
            }
        }
    }

    fn try_http_route_body(
        &mut self,
    ) -> Result<
        (
            Option<AllowedType>,
            Option<AllowedType>,
            String,
            Vec<StatusMapping>,
            Option<AuthRequirement>,
        ),
        String,
    > {
        let mut input = None;
        let mut output = None;
        let mut use_case = None;
        let mut status = Vec::new();
        let mut auth = None;
        let pos = self.current;

        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => break,
                Some(Token::Comma) => continue,
                Some(Token::Keyword(Keyword::Input)) => match self.next_token() {
                    Some(Token::Colon) => input = Some(self.try_type()?),
                    Some(Token::OpenBrace) => {
                        input = Some(self.try_input_block()?);
                    }
                    _ => {
                        self.reset(pos);
                        return Err("Expected ':' or '{' after 'input'".to_string());
                    }
                },
                Some(Token::Keyword(Keyword::Output)) => match self.next_token() {
                    Some(Token::Colon) => output = Some(self.try_type()?),
                    _ => {
                        self.reset(pos);
                        return Err("Expected ':' after 'output'".to_string());
                    }
                },
                Some(Token::Keyword(Keyword::UseCase)) => match self.next_token() {
                    Some(Token::Colon) => match self.next_token() {
                        Some(Token::Identifier(name)) => use_case = Some(name),
                        _ => {
                            self.reset(pos);
                            return Err("Expected identifier after 'use_case:'".to_string());
                        }
                    },
                    _ => {
                        self.reset(pos);
                        return Err("Expected ':' after 'use_case'".to_string());
                    }
                },
                Some(Token::Keyword(Keyword::Status)) => {
                    self.revert(1);
                    status = self.try_status_block()?;
                }
                Some(Token::Keyword(Keyword::Auth)) => {
                    self.revert(1);
                    auth = Some(self.try_auth_block()?);
                }
                _ => {
                    self.reset(pos);
                    return Err("Failed to parse http route body".to_string());
                }
            }
        }
        match use_case {
            Some(u) => Ok((input, output, u, status, auth)),
            None => Err("HTTP route must declare 'use_case'".to_string()),
        }
    }

    fn try_http_route(&mut self) -> Result<HttpRoute, String> {
        let method = self.try_http_method()?;
        match self.next_token() {
            Some(Token::StringLiteral(path)) => match self.next_token() {
                Some(Token::OpenBrace) => {
                    let (input, output, use_case, status, auth) = self.try_http_route_body()?;
                    Ok(HttpRoute {
                        method,
                        path,
                        input,
                        output,
                        use_case,
                        status,
                        auth,
                    })
                }
                _ => {
                    self.revert(1);
                    Err("Expected '{' after route path".to_string())
                }
            },
            _ => {
                self.revert(1);
                Err("Expected string literal path after HTTP method".to_string())
            }
        }
    }

    fn try_http_routes_block(&mut self) -> Result<Vec<HttpRoute>, String> {
        match (self.next_token(), self.next_token()) {
            (Some(Token::Keyword(Keyword::Routes)), Some(Token::OpenBrace)) => {}
            _ => {
                self.revert(2);
                return Err("Expected 'routes {' block".to_string());
            }
        }

        let mut routes = Vec::new();
        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => break,
                _ => {
                    self.revert(1);
                    routes.push(self.try_http_route()?);
                }
            }
        }
        Ok(routes)
    }

    fn try_http_block(&mut self) -> Result<HttpAdapterService, String> {
        match (self.next_token(), self.next_token()) {
            (Some(Token::Keyword(Keyword::Http)), Some(Token::OpenBrace)) => {}
            _ => {
                self.revert(2);
                return Err("Expected 'http {' block".to_string());
            }
        }

        let mut routes = Vec::new();
        let mut on = None;
        let mut cors = None;
        let mut envelope = true;
        let mut events = None;
        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => break,
                Some(Token::Identifier(k)) if k == "envelope" => match (self.next_token(), self.next_token()) {
                    (Some(Token::Colon), Some(Token::Identifier(v))) if v == "true" || v == "false" => {
                        envelope = v == "true"
                    }
                    _ => return Err("Expected ': true' or ': false' after 'envelope'".to_string()),
                },
                Some(Token::Identifier(k)) if k == "events" => {
                    let path = match (self.next_token(), self.next_token()) {
                        (Some(Token::Colon), Some(Token::StringLiteral(path))) => path,
                        _ => return Err("Expected ': \"/path\"' after 'events'".to_string()),
                    };
                    let pos = self.current;
                    let requires_user = match (self.next_token(), self.next_token(), self.next_token()) {
                        (Some(Token::Keyword(Keyword::Auth)), Some(Token::Colon), Some(Token::Identifier(v)))
                            if v == "required" =>
                        {
                            true
                        }
                        _ => {
                            self.reset(pos);
                            false
                        }
                    };
                    events = Some(EventsEndpoint { path, requires_user });
                }
                Some(Token::Keyword(Keyword::Routes)) => {
                    self.revert(1);
                    routes = self.try_http_routes_block()?;
                }
                Some(Token::Identifier(k)) if k == "cors" => {
                    match (self.next_token(), self.next_token()) {
                        (Some(Token::Colon), Some(Token::StringLiteral(origins))) => {
                            cors = Some(origins)
                        }
                        _ => return Err("Expected ': \"<origins>\"' after 'cors'".to_string()),
                    }
                }
                Some(Token::Keyword(Keyword::On)) => match self.next_token() {
                    Some(Token::Colon) => match self.next_token() {
                        Some(Token::StringLiteral(address)) => on = Some(address),
                        _ => {
                            self.revert(1);
                            return Err("Expected string literal after 'on:'".to_string());
                        }
                    },
                    _ => {
                        self.revert(1);
                        return Err("Expected ':' after 'on'".to_string());
                    }
                },
                _ => {
                    self.revert(1);
                    return Err("Unexpected token in 'http' block".to_string());
                }
            }
        }
        Ok(HttpAdapterService { routes, on, cors, envelope, events })
    }

    /// Parses `rpc Name { input: X output: Y use_case: Z }` — the gRPC
    /// equivalent of an HTTP route.
    fn try_grpc_rpc(&mut self) -> Result<GrpcRpc, String> {
        let name = match (self.next_token(), self.next_token(), self.next_token()) {
            (Some(Token::Keyword(Keyword::Rpc)), Some(Token::Identifier(name)), Some(Token::OpenBrace)) => name,
            _ => {
                self.revert(3);
                return Err("Expected 'rpc <Name> {' block".to_string());
            }
        };

        let pos = self.current;
        let mut input = None;
        let mut output = None;
        let mut use_case = None;

        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => break,
                Some(Token::Comma) => continue,
                Some(Token::Keyword(Keyword::Input)) => match self.next_token() {
                    Some(Token::Colon) => input = Some(self.try_type()?),
                    _ => {
                        self.reset(pos);
                        return Err("Expected ':' after 'input'".to_string());
                    }
                },
                Some(Token::Keyword(Keyword::Output)) => match self.next_token() {
                    Some(Token::Colon) => output = Some(self.try_type()?),
                    _ => {
                        self.reset(pos);
                        return Err("Expected ':' after 'output'".to_string());
                    }
                },
                Some(Token::Keyword(Keyword::UseCase)) => match self.next_token() {
                    Some(Token::Colon) => match self.next_token() {
                        Some(Token::Identifier(u)) => use_case = Some(u),
                        _ => {
                            self.reset(pos);
                            return Err("Expected identifier after 'use_case:'".to_string());
                        }
                    },
                    _ => {
                        self.reset(pos);
                        return Err("Expected ':' after 'use_case'".to_string());
                    }
                },
                _ => {
                    self.reset(pos);
                    return Err("Failed to parse 'rpc' body".to_string());
                }
            }
        }

        match use_case {
            Some(use_case) => Ok(GrpcRpc { name, input, output, use_case }),
            None => Err(format!("'rpc {}' must declare 'use_case'", name)),
        }
    }

    fn try_grpc_services_block(&mut self) -> Result<Vec<GrpcRpc>, String> {
        match (self.next_token(), self.next_token()) {
            (Some(Token::Keyword(Keyword::Services)), Some(Token::OpenBrace)) => {}
            _ => {
                self.revert(2);
                return Err("Expected 'services {' block".to_string());
            }
        }

        let mut rpcs = Vec::new();
        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => break,
                _ => {
                    self.revert(1);
                    rpcs.push(self.try_grpc_rpc()?);
                }
            }
        }
        Ok(rpcs)
    }

    fn try_grpc_block(&mut self) -> Result<GrpcAdapterService, String> {
        match (self.next_token(), self.next_token()) {
            (Some(Token::Keyword(Keyword::Grpc)), Some(Token::OpenBrace)) => {}
            _ => {
                self.revert(2);
                return Err("Expected 'grpc {' block".to_string());
            }
        }

        let mut rpcs = Vec::new();
        let mut on = None;
        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => break,
                Some(Token::Keyword(Keyword::Services)) => {
                    self.revert(1);
                    rpcs = self.try_grpc_services_block()?;
                }
                Some(Token::Keyword(Keyword::On)) => match self.next_token() {
                    Some(Token::Colon) => match self.next_token() {
                        Some(Token::StringLiteral(address)) => on = Some(address),
                        _ => {
                            self.revert(1);
                            return Err("Expected string literal after 'on:'".to_string());
                        }
                    },
                    _ => {
                        self.revert(1);
                        return Err("Expected ':' after 'on'".to_string());
                    }
                },
                _ => {
                    self.revert(1);
                    return Err("Unexpected token in 'grpc' block".to_string());
                }
            }
        }
        Ok(GrpcAdapterService { rpcs, on })
    }

    /// Parses `mcp { on: "..." auth ... instructions: "..." tools { ... } }`
    /// (`mcp`, `tools`, `instructions`, `description` are contextual).
    fn try_mcp_block(&mut self) -> Result<McpAdapterService, String> {
        match (self.next_token(), self.next_token()) {
            (Some(Token::Identifier(word)), Some(Token::OpenBrace)) if word == "mcp" => {}
            _ => return Err("Expected 'mcp {' block".to_string()),
        }
        let mut service = McpAdapterService { on: None, auth: None, instructions: None, tools: Vec::new() };
        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => break,
                Some(Token::Comma) => continue,
                Some(Token::Keyword(Keyword::On)) => match (self.next_token(), self.next_token()) {
                    (Some(Token::Colon), Some(Token::StringLiteral(address))) => service.on = Some(address),
                    _ => return Err("Expected ': \"<address>\"' after 'on' in 'mcp'".to_string()),
                },
                Some(Token::Keyword(Keyword::Auth)) => {
                    self.revert(1);
                    service.auth = Some(self.try_auth_block()?);
                }
                Some(Token::Identifier(word)) if word == "instructions" => {
                    match (self.next_token(), self.next_token()) {
                        (Some(Token::Colon), Some(Token::StringLiteral(text))) => service.instructions = Some(text),
                        _ => return Err("Expected ': \"...\"' after 'instructions'".to_string()),
                    }
                }
                Some(Token::Identifier(word)) if word == "tools" => {
                    if self.next_token() != Some(Token::OpenBrace) {
                        return Err("Expected '{' after 'tools'".to_string());
                    }
                    loop {
                        let name = match self.next_token() {
                            Some(Token::CloseBrace) => break,
                            Some(Token::Comma) => continue,
                            Some(Token::Identifier(name)) => name,
                            Some(Token::Keyword(k)) => k.to_string(),
                            other => return Err(format!("Expected a tool name in 'tools', got {:?}", other)),
                        };
                        if self.next_token() != Some(Token::OpenBrace) {
                            return Err(format!("Expected '{{' after tool '{}'", name));
                        }
                        let mut use_case = None;
                        let mut description = None;
                        loop {
                            match self.next_token() {
                                Some(Token::CloseBrace) => break,
                                Some(Token::Comma) => continue,
                                Some(Token::Keyword(Keyword::UseCase)) => match (self.next_token(), self.next_token()) {
                                    (Some(Token::Colon), Some(Token::Identifier(uc))) => use_case = Some(uc),
                                    _ => return Err(format!("Expected ': <UseCase>' in tool '{}'", name)),
                                },
                                Some(Token::Identifier(k)) if k == "description" => match (self.next_token(), self.next_token()) {
                                    (Some(Token::Colon), Some(Token::StringLiteral(text))) => description = Some(text),
                                    _ => return Err(format!("Expected ': \"...\"' after 'description' in tool '{}'", name)),
                                },
                                other => return Err(format!("Unexpected {:?} in tool '{}' (expected use_case, description)", other, name)),
                            }
                        }
                        let use_case = use_case.ok_or(format!("tool '{}' must declare 'use_case'", name))?;
                        if service.tools.iter().any(|t| t.name == name) {
                            return Err(format!("tool '{}' is declared twice", name));
                        }
                        service.tools.push(McpTool { name, use_case, description });
                    }
                }
                other => return Err(format!("Unexpected {:?} in 'mcp' block (expected on, auth, instructions, tools)", other)),
            }
        }
        Ok(service)
    }

    pub(super) fn try_adapter_service(&mut self) -> Result<Ast, String> {
        match (
            self.next_token(),
            self.next_token(),
            self.next_token(),
        ) {
            (
                Some(Token::Keyword(Keyword::Adapter)),
                Some(Token::Identifier(name)),
                Some(Token::OpenBrace),
            ) => match self.next_token() {
                Some(Token::CloseBrace) => Ok(Ast::Adapter(AdapterService {
                    name,
                    adapter_type: AdapterType::Http(HttpAdapterService {
                        routes: vec![],
                        on: None,
                        cors: None,
                        envelope: true,
                        events: None,
                    }),
                })),
                _ => {
                    self.revert(1);
                    let pos = self.current;
                    let first = self.next_token();
                    self.reset(pos);

                    let adapter_type = match first {
                        Some(Token::Keyword(Keyword::Grpc)) => AdapterType::Grpc(self.try_grpc_block()?),
                        Some(Token::Identifier(word)) if word == "mcp" => AdapterType::Mcp(self.try_mcp_block()?),
                        _ => AdapterType::Http(self.try_http_block()?),
                    };

                    match self.next_token() {
                        Some(Token::CloseBrace) => {
                            Ok(Ast::Adapter(AdapterService { name, adapter_type }))
                        }
                        _ => {
                            self.revert(1);
                            Err("Expected '}' to close adapter body".to_string())
                        }
                    }
                }
            },
            _ => {
                self.revert(3);
                Err("Failed to parse AdapterService".to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn parser_for(src: &str) -> Parser {
        let tokens = Lexer::new(&src.to_string()).run().expect("lex should succeed");
        Parser::new(tokens)
    }

    fn parse_adapter(src: &str) -> AdapterService {
        match parser_for(src).try_adapter_service().unwrap() {
            Ast::Adapter(a) => a,
            other => panic!("expected Ast::Adapter, got {:?}", other),
        }
    }

    #[test]
    fn empty_adapter_body_defaults_to_empty_http() {
        let adapter = parse_adapter("adapter Empty {}");
        assert_eq!(adapter.name, "Empty");
        match adapter.adapter_type {
            AdapterType::Http(http) => {
                assert!(http.routes.is_empty());
                assert!(http.on.is_none());
            }
            other => panic!("expected AdapterType::Http, got {:?}", other),
        }
    }

    #[test]
    fn input_colon_and_input_block_forms_produce_the_same_type() {
        let colon_form = parse_adapter(
            r#"adapter A {
                http {
                    routes {
                        POST "/users" {
                            input: String
                            output: String
                            use_case: CreateUser
                        }
                    }
                }
            }"#,
        );
        let block_form = parse_adapter(
            r#"adapter A {
                http {
                    routes {
                        POST "/users" {
                            input { body: String }
                            output: String
                            use_case: CreateUser
                        }
                    }
                }
            }"#,
        );
        let extract_input = |a: AdapterService| match a.adapter_type {
            AdapterType::Http(http) => http.routes[0].input.clone(),
            _ => panic!("expected http adapter"),
        };
        assert_eq!(extract_input(colon_form), Some(AllowedType::String));
        assert_eq!(extract_input(block_form), Some(AllowedType::String));
    }

    #[test]
    fn input_block_form_is_not_accepted_on_a_grpc_rpc() {
        // Pins the current limitation: `input { body: Type }` is only wired
        // up for HTTP routes, not gRPC `rpc` bodies.
        let src = r#"adapter A {
            grpc {
                services {
                    rpc DoThing {
                        input { body: String }
                        output: String
                        use_case: DoThing
                    }
                }
            }
        }"#;
        assert!(parser_for(src).try_adapter_service().is_err());
    }

    #[test]
    fn http_route_requires_use_case() {
        let src = r#"adapter A {
            http {
                routes {
                    GET "/users" {
                        output: String
                    }
                }
            }
        }"#;
        let err = parser_for(src).try_adapter_service().unwrap_err();
        assert_eq!(err, "HTTP route must declare 'use_case'");
    }

    #[test]
    fn every_http_method_parses() {
        for (word, expected) in [
            ("GET", HttpMethod::Get),
            ("POST", HttpMethod::Post),
            ("PUT", HttpMethod::Put),
            ("PATCH", HttpMethod::Patch),
            ("DELETE", HttpMethod::Delete),
        ] {
            let src = format!(
                r#"adapter A {{
                    http {{
                        routes {{
                            {} "/x" {{
                                use_case: X
                            }}
                        }}
                    }}
                }}"#,
                word
            );
            let adapter = parse_adapter(&src);
            match adapter.adapter_type {
                AdapterType::Http(http) => assert_eq!(http.routes[0].method, expected),
                other => panic!("expected AdapterType::Http, got {:?}", other),
            }
        }
    }

    #[test]
    fn on_address_parses_for_http_adapter() {
        let adapter = parse_adapter(
            r#"adapter A {
                http {
                    on: "0.0.0.0:8080"
                    routes {}
                }
            }"#,
        );
        match adapter.adapter_type {
            AdapterType::Http(http) => assert_eq!(http.on, Some("0.0.0.0:8080".to_string())),
            other => panic!("expected AdapterType::Http, got {:?}", other),
        }
    }

    #[test]
    fn status_block_with_and_without_multiple_mappings_parses() {
        let adapter = parse_adapter(
            r#"adapter A {
                http {
                    routes {
                        GET "/x" {
                            use_case: X
                            status {
                                NotFound -> 404
                                Conflict -> 409
                            }
                        }
                    }
                }
            }"#,
        );
        match adapter.adapter_type {
            AdapterType::Http(http) => {
                let status = &http.routes[0].status;
                assert_eq!(status.len(), 2);
                assert_eq!(status[0].target, "NotFound");
                assert_eq!(status[0].status, 404);
                assert_eq!(status[1].target, "Conflict");
                assert_eq!(status[1].status, 409);
            }
            other => panic!("expected AdapterType::Http, got {:?}", other),
        }
    }

    #[test]
    fn auth_block_present_and_absent() {
        let with_auth = parse_adapter(
            r#"adapter A {
                http {
                    routes {
                        GET "/x" {
                            use_case: X
                            auth {
                                infra: Perms
                                target: "user"
                                action: "read"
                            }
                        }
                    }
                }
            }"#,
        );
        match with_auth.adapter_type {
            AdapterType::Http(http) => {
                let auth = match http.routes[0]
                    .auth
                    .as_ref()
                    .expect("expected an auth requirement")
                {
                    AuthRequirement::Permission(p) => p,
                    other => panic!("expected a permission requirement, got {:?}", other),
                };
                assert_eq!(auth.infra, "Perms");
                assert_eq!(auth.target, "user");
                assert_eq!(auth.action, "read");
            }
            other => panic!("expected AdapterType::Http, got {:?}", other),
        }

        let without_auth = parse_adapter(
            r#"adapter A {
                http {
                    routes {
                        GET "/x" {
                            use_case: X
                        }
                    }
                }
            }"#,
        );
        match without_auth.adapter_type {
            AdapterType::Http(http) => assert!(http.routes[0].auth.is_none()),
            other => panic!("expected AdapterType::Http, got {:?}", other),
        }
    }

    #[test]
    fn http_block_parses_cors_origins() {
        let adapter = parse_adapter(
            r#"adapter A {
                http {
                    on: "0.0.0.0:8080"
                    cors: "${CORS_ORIGINS}"
                }
            }"#,
        );
        match adapter.adapter_type {
            AdapterType::Http(http) => assert_eq!(http.cors.as_deref(), Some("${CORS_ORIGINS}")),
            other => panic!("expected AdapterType::Http, got {:?}", other),
        }
    }

    #[test]
    fn cors_without_a_string_is_an_error() {
        let result = parser_for(
            r#"adapter A {
                http {
                    cors: CORS_ORIGINS
                }
            }"#,
        )
        .try_adapter_service();
        assert!(result.is_err());
    }

    #[test]
    fn auth_required_shorthand_parses_as_authenticated() {
        let adapter = parse_adapter(
            r#"adapter A {
                http {
                    routes {
                        GET "/me" {
                            use_case: X
                            auth: required
                        }
                    }
                }
            }"#,
        );
        match adapter.adapter_type {
            AdapterType::Http(http) => {
                assert!(matches!(
                    http.routes[0].auth,
                    Some(AuthRequirement::Authenticated)
                ))
            }
            other => panic!("expected AdapterType::Http, got {:?}", other),
        }
    }

    #[test]
    fn auth_colon_with_anything_but_required_is_an_error() {
        let result = parser_for(
            r#"adapter A {
                http {
                    routes {
                        GET "/me" {
                            use_case: X
                            auth: optional
                        }
                    }
                }
            }"#,
        )
        .try_adapter_service();
        assert!(result.is_err());
    }

    #[test]
    fn grpc_rpc_parses_input_output_and_use_case() {
        let adapter = parse_adapter(
            r#"adapter A {
                grpc {
                    on: "0.0.0.0:50051"
                    services {
                        rpc GetUser {
                            input: String
                            output: String
                            use_case: GetUser
                        }
                    }
                }
            }"#,
        );
        match adapter.adapter_type {
            AdapterType::Grpc(grpc) => {
                assert_eq!(grpc.on, Some("0.0.0.0:50051".to_string()));
                assert_eq!(grpc.rpcs.len(), 1);
                assert_eq!(grpc.rpcs[0].name, "GetUser");
                assert_eq!(grpc.rpcs[0].input, Some(AllowedType::String));
                assert_eq!(grpc.rpcs[0].output, Some(AllowedType::String));
                assert_eq!(grpc.rpcs[0].use_case, "GetUser");
            }
            other => panic!("expected AdapterType::Grpc, got {:?}", other),
        }
    }

    #[test]
    fn grpc_rpc_requires_use_case() {
        let src = r#"adapter A {
            grpc {
                services {
                    rpc GetUser {
                        input: String
                    }
                }
            }
        }"#;
        assert!(parser_for(src).try_adapter_service().is_err());
    }
}
