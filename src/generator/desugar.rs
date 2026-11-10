//! Rewrites shorthand declarations into the ones the generator knows,
//! before anything else looks at the project:
//!
//! - `resource` blocks become CRUD use cases, plus routes in every HTTP
//!   adapter listing them under `resources {}`;
//! - a psql infra's `owner:` becomes the `owner:` of every `call:` use case
//!   whose primary call targets it (`owner: none` opts out).

use std::collections::HashMap;

use crate::parser::{
    AdapterType, AllowedType, Ast, HttpMethod, HttpRoute, InfraServiceType, ResourceDeclaration,
    StorageType, UseCaseCall, UseCaseService,
};

fn use_case(name: String, infra: &str, method: &str, owner: Option<String>) -> UseCaseService {
    UseCaseService {
        name,
        input: None,
        output: None,
        calls: vec![UseCaseCall { target: infra.to_string(), method: method.to_string(), errors: Vec::new() }],
        ai: None,
        blob: None,
        filters: Vec::new(),
        owner,
        code: None,
        order_by: Vec::new(),
        transaction: false,
        notify: None,
    }
}

/// The use cases and routes (with no auth yet) of one resource.
fn expand(resource: &ResourceDeclaration, entity: &str) -> (Vec<UseCaseService>, Vec<HttpRoute>) {
    let base = resource.path.trim_end_matches('/');
    let item = format!("{}/:id", base);
    let mut use_cases = Vec::new();
    let mut routes = Vec::new();
    for op in &resource.operations {
        let (name, method, http, path) = match op.kind.as_str() {
            "list" => (format!("List{}UseCase", resource.name), "list", HttpMethod::Get, base.to_string()),
            "get" => (format!("Get{}UseCase", entity), "find_one", HttpMethod::Get, item.clone()),
            "create" => (format!("Create{}UseCase", entity), "save_one", HttpMethod::Post, base.to_string()),
            "upsert" => (format!("Upsert{}UseCase", entity), "upsert_one", HttpMethod::Put, base.to_string()),
            "update" => (format!("Update{}UseCase", entity), "update_one", HttpMethod::Put, item.clone()),
            "patch" => (format!("Patch{}UseCase", entity), "patch_one", HttpMethod::Patch, item.clone()),
            _ => (format!("Delete{}UseCase", entity), "delete", HttpMethod::Delete, item.clone()),
        };
        let mut uc = use_case(name.clone(), &resource.infra, method, resource.owner.clone());
        uc.filters = op.filters.clone();
        uc.order_by = op.order_by.clone();
        use_cases.push(uc);
        routes.push(HttpRoute {
            method: http,
            path: if path.is_empty() { "/".to_string() } else { path },
            input: None,
            output: None,
            use_case: name,
            status: Vec::new(),
            auth: None,
            auth_explicit: false,
        });
    }
    (use_cases, routes)
}

pub fn desugar(asts: Vec<Ast>) -> Result<Vec<Ast>, String> {
    let mut infra_entities: HashMap<String, String> = HashMap::new();
    let mut infra_owners: HashMap<String, String> = HashMap::new();
    for ast in &asts {
        if let Ast::Infra(infra) = ast {
            if let InfraServiceType::Storage(StorageType::Psql(psql)) = &infra.service_type {
                if let AllowedType::DefinedType(entity) = &psql.entity {
                    infra_entities.insert(infra.name.clone(), entity.clone());
                }
                if let Some(owner) = &psql.owner {
                    infra_owners.insert(infra.name.clone(), owner.clone());
                }
            }
        }
    }

    let mut resource_routes: HashMap<String, Vec<HttpRoute>> = HashMap::new();
    let mut out: Vec<Ast> = Vec::new();
    for ast in asts {
        match ast {
            Ast::Resource(resource) => {
                let entity = infra_entities.get(&resource.infra).ok_or_else(|| {
                    format!("resource '{}': '{}' is not a psql infra", resource.name, resource.infra)
                })?;
                let (use_cases, routes) = expand(&resource, entity);
                out.extend(use_cases.into_iter().map(Ast::UseCase));
                resource_routes.insert(resource.name.clone(), routes);
            }
            other => out.push(other),
        }
    }

    for ast in &mut out {
        match ast {
            Ast::Adapter(adapter) => {
                if let AdapterType::Http(http) = &mut adapter.adapter_type {
                    for name in &http.resources {
                        let routes = resource_routes.get(name).ok_or_else(|| {
                            format!("adapter '{}' lists unknown resource '{}'", adapter.name, name)
                        })?;
                        for route in routes {
                            let mut route = route.clone();
                            route.auth = http.default_auth.clone();
                            http.routes.push(route);
                        }
                    }
                }
            }
            Ast::UseCase(use_case) => {
                if use_case.owner.as_deref() == Some("none") {
                    use_case.owner = None;
                } else if use_case.owner.is_none() && use_case.code.is_none() {
                    if let Some(owner) = use_case.calls.last().and_then(|c| infra_owners.get(&c.target)) {
                        use_case.owner = Some(owner.clone());
                    }
                }
            }
            _ => {}
        }
    }
    Ok(out)
}
