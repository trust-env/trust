use std::{
    fs::{File, create_dir_all},
    io::Write,
    path::PathBuf,
};

use heck::ToSnakeCase;

use crate::{
    generator::rust_code::{
        dev_docs, env_template,
        imports::{LocalImports, common_use, crate_use, use_group, use_statement},
        fragments::{mod_decls, params as fragment_params, shared_bound, where_clause},
        render::render,
    },
    parser::{ErrorMapping, HttpMethod, StatusMapping},
};
use serde::Serialize;
use tera::Context;

/// A route's resolved `auth { infra: ... target: "..." action: "..." }`
/// requirement: before the use case runs, the caller's JWT (via
/// `auth::UserContext`) is checked against `infra_name`'s storage for a
/// permission row matching `target`/`action`.
pub struct ResolvedAuth {
    pub infra_name: String,
    pub storage_trait: String,
    pub target: String,
    pub action: String,
}

pub struct HttpRouteBinding {
    pub method: HttpMethod,
    pub path: String,
    pub use_case_name: String,
    pub use_case_field: String,
    pub type_param: String,
    /// The use case's own `errors {}` mappings — only used here to know
    /// which custom variant names exist on its error enum. The use case's
    /// error enum stays HTTP-agnostic; it never carries a status code.
    pub error_mappings: Vec<ErrorMapping>,
    /// The route's own `status {}` block — this is what actually decides
    /// the HTTP status for each of the use case's error variants.
    pub status_mappings: Vec<StatusMapping>,
    /// The actual Rust type name to reference for the input/output:
    /// `{UseCase}Input`/`{UseCase}Output` for CRUD use cases, or the
    /// declared type's own name for `ai {}` use cases.
    pub input_type_name: String,
    pub output_type_name: String,
    /// Field docs for `--dev`'s `/app` page. Empty when the use case's shape
    /// couldn't be resolved (shouldn't normally happen by the time routes
    /// are generated, but docs are best-effort, not load-bearing).
    pub input_fields: Vec<(String, String)>,
    pub output_entity: String,
    pub output_fields: Vec<(String, String)>,
    pub output_is_list: bool,
    /// Names of `:name` segments captured from the route path — these
    /// fields of the use case's input come from the URL, not the JSON body.
    pub path_params: Vec<String>,
    /// Any `auth` form: the handler extracts `auth::UserContext` (401 on a
    /// missing/invalid JWT) and passes its `user_id` to `execute`.
    pub requires_user: bool,
    /// The use case takes the built-in `RawRequest`: the handler passes the
    /// untouched body and headers instead of decoding JSON/query fields.
    pub raw_input: bool,
    /// Only the permission form (`auth { infra target action }`).
    pub auth: Option<ResolvedAuth>,
}

#[derive(Serialize)]
struct StructFieldView {
    name: String,
    field_type: String,
}

enum InputExtraction<'a> {
    /// No `:name` segments in the path — the whole input comes from the
    /// JSON body, exactly as before this feature existed.
    BodyOnly,
    /// Every input field is captured from the path — no JSON body at all.
    PathOnly,
    /// Some fields come from the path, the rest from the JSON body.
    Split {
        path_fields: Vec<&'a (String, String)>,
        body_fields: Vec<&'a (String, String)>,
    },
}

pub struct HttpAdapterFile {
    name: String,
    routes: Vec<HttpRouteBinding>,
    routes_dir: PathBuf,
    dev_mode: bool,
    cors: Option<String>,
    file: File,
}

impl HttpAdapterFile {
    pub fn new(
        name: String,
        routes: Vec<HttpRouteBinding>,
        dev_mode: bool,
        cors: Option<String>,
        folder: PathBuf,
    ) -> Result<Self, String> {
        let file_stem = name.to_snake_case();
        let routes_dir = folder.join(&file_stem);
        create_dir_all(&routes_dir)
            .map_err(|e| format!("Failed to create routes folder {:?}: {}", routes_dir, e))?;

        let file_path = folder.join(format!("{}.rs", file_stem));

        Ok(HttpAdapterFile {
            file: File::create(&file_path)
                .map_err(|e| format!("Failed to create adapter file: {}", e))?,
            name,
            routes,
            routes_dir,
            dev_mode,
            cors,
        })
    }

    fn unique_bindings(&self) -> Vec<&HttpRouteBinding> {
        let mut seen = Vec::new();
        let mut result = Vec::new();

        for route in &self.routes {
            if !seen.contains(&route.type_param) {
                seen.push(route.type_param.clone());
                result.push(route);
            }
        }
        result
    }

    /// Auth infra bindings, deduped by infra name across every route that
    /// declares an `auth` requirement — each distinct infra gets its own
    /// generic storage param/field on the adapter struct, named `A{i}`
    /// (separate from the use case params `C{i}`).
    fn unique_auth_bindings(&self) -> Vec<&ResolvedAuth> {
        let mut seen = Vec::new();
        let mut result = Vec::new();

        for route in &self.routes {
            if let Some(auth) = &route.auth {
                if !seen.contains(&auth.infra_name) {
                    seen.push(auth.infra_name.clone());
                    result.push(auth);
                }
            }
        }
        result
    }

    fn auth_field_name(auth: &ResolvedAuth) -> String {
        auth.infra_name.to_snake_case()
    }

    fn generics(&self) -> String {
        let mut parts: Vec<String> = self.unique_bindings().iter().map(|b| b.type_param.clone()).collect();
        parts.extend((0..self.unique_auth_bindings().len()).map(|i| format!("A{}", i)));
        parts.join(", ")
    }

    fn where_clause(&self) -> String {
        let mut bounds: Vec<(String, Vec<String>)> = self
            .unique_bindings()
            .iter()
            .map(|b| (b.type_param.clone(), shared_bound(&format!("{}Trait", b.use_case_name))))
            .collect();
        for (i, auth) in self.unique_auth_bindings().iter().enumerate() {
            bounds.push((format!("A{}", i), shared_bound(&auth.storage_trait)));
        }
        where_clause(&bounds)
    }

    fn handler_name(&self, route: &HttpRouteBinding) -> String {
        format!("{}_{}", route.use_case_field, route.method.as_axum_fn())
    }

    fn route_file_path(&self, route: &HttpRouteBinding) -> PathBuf {
        self.routes_dir.join(format!("{}.rs", self.handler_name(route)))
    }

    fn gen_main_imports(&self) -> String {
        let mut axum_fns: Vec<&str> = self.routes.iter().map(|r| r.method.as_axum_fn()).collect();
        if self.dev_mode {
            axum_fns.push("get");
        }
        axum_fns.sort();
        axum_fns.dedup();

        let mut code = String::new();
        code.push_str(&use_statement("std::sync", &["Arc"]));
        code.push('\n');
        code.push_str(&use_statement("axum", &["Router", &use_group("routing", &axum_fns)]));
        code.push_str(&common_use(&["HttpPresenter"]));

        let mut crate_items = Vec::new();
        for binding in self.unique_bindings() {
            crate_items.push(format!("{}Trait", binding.use_case_name));
        }
        for auth in self.unique_auth_bindings() {
            crate_items.push(auth.storage_trait.clone());
        }
        code.push_str(&crate_use(&crate_items));
        code.push('\n');
        code
    }

    fn gen_mods(&self) -> String {
        let mut mods: Vec<String> = self.routes.iter().map(|route| self.handler_name(route)).collect();
        if self.dev_mode {
            mods.push("app_docs".to_string());
        }
        let mut code = mod_decls(&mods);
        code.push('\n');
        code
    }

    fn gen_struct(&self) -> String {
        #[derive(Serialize)]
        struct UseCaseFieldView {
            field: String,
            type_param: String,
        }
        #[derive(Serialize)]
        struct AuthFieldView {
            field: String,
            index: usize,
        }

        let use_case_fields: Vec<UseCaseFieldView> = self
            .unique_bindings()
            .iter()
            .map(|b| UseCaseFieldView {
                field: b.use_case_field.clone(),
                type_param: b.type_param.clone(),
            })
            .collect();
        let auth_fields: Vec<AuthFieldView> = self
            .unique_auth_bindings()
            .iter()
            .enumerate()
            .map(|(i, auth)| AuthFieldView {
                field: Self::auth_field_name(auth),
                index: i,
            })
            .collect();

        let mut context = Context::new();
        context.insert("name", &self.name);
        context.insert("generics", &self.generics());
        context.insert("where_clause", &self.where_clause());
        context.insert("use_case_fields", &use_case_fields);
        context.insert("auth_fields", &auth_fields);
        render("file/http_adapter/struct.rs.tera", &context)
    }

    fn gen_ctor(&self) -> String {
        let mut params: Vec<(String, String)> = self
            .unique_bindings()
            .iter()
            .map(|b| (b.use_case_field.clone(), b.type_param.clone()))
            .collect();
        for (i, auth) in self.unique_auth_bindings().iter().enumerate() {
            params.push((Self::auth_field_name(auth), format!("A{}", i)));
        }
        let params = fragment_params(&params);

        let mut fields: Vec<String> = self.unique_bindings().iter().map(|b| b.use_case_field.clone()).collect();
        for auth in self.unique_auth_bindings() {
            fields.push(Self::auth_field_name(auth));
        }

        let mut context = Context::new();
        context.insert("generics", &self.generics());
        context.insert("name", &self.name);
        context.insert("where_clause", &self.where_clause());
        context.insert("params", &params);
        context.insert("fields", &fields);
        render("file/http_adapter/ctor.rs.tera", &context)
    }

    fn gen_router(&self) -> String {
        #[derive(Serialize)]
        struct RouteLineView {
            path: String,
            axum_fn: String,
            handler: String,
        }

        let route_lines: Vec<RouteLineView> = self
            .routes
            .iter()
            .map(|route| RouteLineView {
                path: crate::parser::to_axum_path(&route.path),
                axum_fn: route.method.as_axum_fn().to_string(),
                handler: self.handler_name(route),
            })
            .collect();

        let mut context = Context::new();
        context.insert("generics", &self.generics());
        context.insert("name", &self.name);
        context.insert("where_clause", &self.where_clause());
        context.insert("route_lines", &route_lines);
        context.insert("dev_mode", &self.dev_mode);
        context.insert(
            "cors_expr",
            &self.cors.as_deref().map(env_template::to_rust_expr),
        );
        let mut code = render("file/http_adapter/router.rs.tera", &context);
        if self.cors.is_some() {
            code.push_str(&render("file/http_adapter/cors.rs.tera", &Context::new()));
        }
        code
    }

    /// How a route's use_case input is assembled from the request: entirely
    /// from the JSON body (the default), entirely from `:name` path
    /// captures (when every input field is captured), or split between the
    /// two.
    fn input_extraction(route: &HttpRouteBinding) -> InputExtraction<'_> {
        if route.path_params.is_empty() {
            return InputExtraction::BodyOnly;
        }

        let path_fields: Vec<_> = route
            .input_fields
            .iter()
            .filter(|(name, _)| route.path_params.contains(name))
            .collect();
        let body_fields: Vec<_> = route
            .input_fields
            .iter()
            .filter(|(name, _)| !route.path_params.contains(name))
            .collect();

        if body_fields.is_empty() {
            InputExtraction::PathOnly
        } else {
            InputExtraction::Split { path_fields, body_fields }
        }
    }

    fn path_struct_name(route: &HttpRouteBinding) -> String {
        format!("{}PathParams", route.use_case_name)
    }

    fn body_struct_name(route: &HttpRouteBinding) -> String {
        format!("{}Body", route.use_case_name)
    }

    fn struct_field_views(fields: &[&(String, String)]) -> Vec<StructFieldView> {
        fields
            .iter()
            .map(|(name, ty)| StructFieldView {
                name: name.clone(),
                field_type: ty.clone(),
            })
            .collect()
    }

    /// Emits the two small per-route structs needed when a use_case's input
    /// is split between path and body: axum's `Path<T>` extractor needs a
    /// type with only the captured fields, and the JSON body must not
    /// require the fields the path already supplies.
    fn gen_split_structs(
        route: &HttpRouteBinding,
        path_fields: &[&(String, String)],
        body_fields: &[&(String, String)],
    ) -> String {
        let mut context = Context::new();
        context.insert("path_name", &Self::path_struct_name(route));
        context.insert("path_fields", &Self::struct_field_views(path_fields));
        context.insert("body_name", &Self::body_struct_name(route));
        context.insert("body_fields", &Self::struct_field_views(body_fields));
        render("file/http_adapter/split_structs.rs.tera", &context)
    }

    /// When the route locally declares a `{Name}PathParams`/`{Name}Body`
    /// struct (i.e. `input_extraction` isn't `BodyOnly` — see
    /// `gen_split_structs`), those structs echo each field's type string
    /// verbatim. Unlike the crate-level `{Name}Input` struct (whose own
    /// file's `LocalImports` already handles this), nothing else imports
    /// for *this* file's locally-declared structs — so a `Uuid`/`Decimal`/
    /// other non-primitive field type here needs its own import, or the
    /// generated route file fails with "cannot find type" the moment any
    /// path/body field isn't a plain `String`/`Int`/`Float`.
    fn local_struct_imports(route: &HttpRouteBinding) -> LocalImports {
        let mut imports = LocalImports::default();
        if !route.path_params.is_empty() {
            for (_, ty) in &route.input_fields {
                imports.add_type_string(ty);
            }
        }
        imports
    }

    fn route_file_imports(&self, route: &HttpRouteBinding) -> String {
        let mut code = String::new();

        code.push_str(&use_statement("std::sync", &["Arc"]));
        code.push('\n');
        let mut axum_items = vec!["Json", "extract::State", "http::StatusCode"];
        if !route.path_params.is_empty() {
            axum_items.push("extract::Path");
        }
        // `Json` (the response type, and the request body extractor for
        // every non-GET route) is already unconditional above — only a GET
        // route with a non-path-only input needs the extra `Query` import,
        // since it reads those fields from the query string instead (see
        // `body_extractor`'s doc comment for why).
        if route.method == HttpMethod::Get && !matches!(Self::input_extraction(route), InputExtraction::PathOnly) {
            axum_items.push("extract::Query");
        }
        code.push_str(&use_statement("axum", &axum_items));
        if route.requires_user {
            code.push_str(&use_statement("auth", &["UserContext"]));
        }
        if route.auth.is_some() {
            code.push_str(&common_use(&[
                "HttpError",
                "HttpResult",
                "StorageListQuery",
                "StoragePredicate",
                "StoragePredicateOperator",
            ]));
        } else {
            code.push_str(&common_use(&["HttpError", "HttpResult"]));
        }
        code.push_str(&use_statement("super", &[&self.name]));

        let local = Self::local_struct_imports(route);
        code.push_str(&local.external_use_statements());

        let mut crate_items = local.types;
        for binding in self.unique_bindings() {
            crate_items.push(format!("{}Trait", binding.use_case_name));
        }
        for auth in self.unique_auth_bindings() {
            crate_items.push(auth.storage_trait.clone());
        }
        crate_items.push(route.input_type_name.clone());
        crate_items.push(route.output_type_name.clone());
        crate_items.push(format!("{}Error", route.use_case_name));
        crate_items.sort();
        crate_items.dedup();
        code.push_str(&crate_use(&crate_items));
        code.push('\n');
        code
    }

    /// The permission check that runs before the use case, when the route
    /// declares an `auth` requirement: looks up whether the JWT's user_id
    /// has a permission row matching `target`/`action` on the given infra,
    /// rejecting with 403 if not (500 if the lookup itself fails).
    fn gen_auth_prelude(route: &HttpRouteBinding) -> String {
        match &route.auth {
            None => String::new(),
            Some(auth) => {
                let mut context = Context::new();
                context.insert("field", &Self::auth_field_name(auth));
                context.insert("target_literal", &format!("{:?}", auth.target));
                context.insert("action_literal", &format!("{:?}", auth.action));
                render("file/http_adapter/auth_prelude.rs.tera", &context)
            }
        }
    }

    fn unique_error_targets(mappings: &[ErrorMapping]) -> Vec<&str> {
        let mut seen = Vec::new();

        for mapping in mappings {
            if !seen.contains(&mapping.target.as_str()) {
                seen.push(mapping.target.as_str());
            }
        }
        seen
    }

    /// Looks up the status for a specific error target, falling back to the
    /// route's `_ -> N` wildcard (if declared) and finally to 500.
    fn target_status(mappings: &[StatusMapping], target: &str) -> u16 {
        mappings
            .iter()
            .find(|m| m.target == target)
            .or_else(|| mappings.iter().find(|m| m.target == "_"))
            .map(|m| m.status)
            .unwrap_or(500)
    }

    /// The use case's error enum is HTTP-agnostic (`NotFound`, `StorageError`,
    /// plus whatever custom variants its `errors {}` block introduced) — the
    /// adapter is what decides which status code each of those maps to.
    fn gen_error_status_fn(&self, route: &HttpRouteBinding) -> String {
        #[derive(Serialize)]
        struct TargetView {
            target: String,
            status: u16,
        }

        let fn_name = format!("{}_status", self.handler_name(route));
        let error_type = format!("{}Error", route.use_case_name);
        let targets: Vec<TargetView> = Self::unique_error_targets(&route.error_mappings)
            .into_iter()
            .map(|target| TargetView {
                target: target.to_string(),
                status: Self::target_status(&route.status_mappings, target),
            })
            .collect();

        let mut context = Context::new();
        context.insert("fn_name", &fn_name);
        context.insert("error_type", &error_type);
        context.insert("targets", &targets);
        render("file/http_adapter/error_status_fn.rs.tera", &context)
    }

    /// Which axum extractor reads a route's non-path input fields: `Json`
    /// (request body) for every method except `GET`, and `Query` (URL query
    /// string) for `GET` — a real HTTP client's `fetch()`/`XMLHttpRequest`
    /// refuses to send a body on a GET request at all (verified: `fetch()`
    /// throws `TypeError: Request with GET/HEAD method cannot have body`
    /// before the request is even sent), so a `GET` route that put
    /// `limit`/`offset`/`filters {}` fields in a JSON body would be
    /// unreachable from any standard client, not just an awkward API shape.
    fn body_extractor(route: &HttpRouteBinding) -> &'static str {
        if route.method == HttpMethod::Get { "Query" } else { "Json" }
    }

    fn route_file_content(&self, route: &HttpRouteBinding) -> String {
        let mut code = self.route_file_imports(route);

        code.push_str(&self.gen_error_status_fn(route));

        #[derive(Serialize)]
        struct SplitFieldView<'a> {
            name: &'a str,
            source: &'static str,
        }

        let mut params_context = Context::new();
        params_context.insert("requires_user", &route.requires_user);
        params_context.insert("extractor", Self::body_extractor(route));
        params_context.insert("input_type", &route.input_type_name);
        let (pre_structs, prelude) = if route.raw_input {
            // `String` consumes the body, so it must be the last extractor.
            params_context.insert("kind", "raw");
            (String::new(), render("file/http_adapter/raw_request.rs.tera", &Context::new()))
        } else {
            match Self::input_extraction(route) {
                InputExtraction::BodyOnly => {
                    params_context.insert("kind", "body");
                    (String::new(), String::new())
                }
                InputExtraction::PathOnly => {
                    params_context.insert("kind", "path");
                    (String::new(), String::new())
                }
                InputExtraction::Split {
                    path_fields,
                    body_fields,
                } => {
                    params_context.insert("kind", "split");
                    params_context.insert("path_name", &Self::path_struct_name(route));
                    params_context.insert("body_name", &Self::body_struct_name(route));
                    let structs = Self::gen_split_structs(route, &path_fields, &body_fields);
                    let fields: Vec<SplitFieldView> = route
                        .input_fields
                        .iter()
                        .map(|(name, _)| SplitFieldView {
                            name,
                            source: if route.path_params.contains(name) { "path" } else { "body" },
                        })
                        .collect();
                    let mut context = Context::new();
                    context.insert("input_type", &route.input_type_name);
                    context.insert("fields", &fields);
                    (structs, render("file/http_adapter/split_input.rs.tera", &context))
                }
            }
        };
        let full_params = render("file/http_adapter/handler_params.rs.tera", &params_context);

        let mut full_prelude = Self::gen_auth_prelude(route);
        full_prelude.push_str(&prelude);

        code.push_str(&pre_structs);

        let mut context = Context::new();
        context.insert("generics", &self.generics());
        context.insert("name", &self.name);
        context.insert("where_clause", &self.where_clause());
        context.insert("handler", &self.handler_name(route));
        context.insert("full_params", &full_params);
        context.insert("output_type", &route.output_type_name);
        context.insert("full_prelude", &full_prelude);
        context.insert("field", &route.use_case_field);
        context.insert(
            "user_expr",
            if route.requires_user {
                "user_context.user_id"
            } else {
                "0"
            },
        );
        context.insert("status_fn", &format!("{}_status", self.handler_name(route)));
        code.push_str(&render("file/http_adapter/handler.rs.tera", &context));
        code
    }

    fn write_dev_docs(&self) -> Result<(), String> {
        let html = dev_docs::render(&self.name, &self.routes);
        let mut context = Context::new();
        context.insert("html", &html);
        let content = render("file/http_adapter/app_docs.rs.tera", &context);
        std::fs::write(self.routes_dir.join("app_docs.rs"), content)
            .map_err(|e| format!("Failed to write dev docs route file: {}", e))
    }

    pub fn write(&mut self) -> Result<(), String> {
        for route in &self.routes {
            let content = self.route_file_content(route);
            std::fs::write(self.route_file_path(route), content)
                .map_err(|e| format!("Failed to write route file: {}", e))?;
        }
        if self.dev_mode {
            self.write_dev_docs()?;
        }

        let mut code = String::new();
        code.push_str(&self.gen_main_imports());
        code.push_str(&self.gen_mods());
        code.push_str(&self.gen_struct());
        code.push_str(&self.gen_ctor());
        code.push_str(&self.gen_router());
        self.file
            .write_all(code.as_bytes())
            .map_err(|e| format!("Failed to write to adapter file: {}", e))
    }
}
