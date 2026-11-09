use std::collections::HashSet;

use crate::parser::{
    AdapterService, AdapterType, AllowedType, AppService, Ast, InfraService, InfraServiceType,
    InfraServiceType::Storage, StorageType, StorageType::Psql, TypeDeclaration, UseCaseService,
};

/// Splits a flat parsed file into one self-contained `Vec<Ast>` per named
/// `app` block: each app's slice contains only the `Adapter`/`UseCase`/
/// `Infra`/`TypeDecl` nodes transitively reachable from its own `run { }`
/// block, so each app can be generated into its own independent crate.
/// `Import`/`Plugin` nodes pass through unfiltered into every app (cheap
/// simplification — files have at most a couple of these today).
///
/// Unresolvable names are simply left out of the closure rather than
/// erroring here: the existing `find_adapter`/`find_use_case_infra` lookups
/// inside `generate_app` already raise the authoritative "unknown X"
/// error, now against the filtered subset, so this pass doesn't need a
/// second copy of that error reporting.
pub(super) fn partition_by_app(asts: &[Ast]) -> Result<Vec<(AppService, Vec<Ast>)>, String> {
    let apps: Vec<&AppService> = asts
        .iter()
        .filter_map(|ast| match ast {
            Ast::App(app) => Some(app),
            _ => None,
        })
        .collect();

    let mut seen_names: HashSet<&str> = HashSet::new();
    for app in &apps {
        if !seen_names.insert(app.name.as_str()) {
            return Err(format!("Duplicate app name '{}'", app.name));
        }
    }

    Ok(apps.into_iter().map(|app| (app.clone(), reachable_asts(asts, app))).collect())
}

fn reachable_asts(asts: &[Ast], app: &AppService) -> Vec<Ast> {
    // `load_balancer {}` no longer references a local adapter at all — its
    // `targets` are real network addresses of independently deployed
    // processes (typically a different app entirely), so there's nothing
    // of its own to add to this app's own reachable adapter set.
    let adapter_names: HashSet<String> = app.adapters.iter().cloned().collect();

    let mut use_case_names: HashSet<String> = HashSet::new();
    let mut infra_names: HashSet<String> = HashSet::new();

    // A `schedule` listed in `run {}` brings in the use cases it runs.
    let mut schedule_names: HashSet<String> = HashSet::new();
    for name in &adapter_names {
        if let Some(schedule) = find_schedule(asts, name) {
            schedule_names.insert(name.clone());
            for job in &schedule.jobs {
                use_case_names.insert(job.use_case.clone());
            }
        }
    }

    for adapter_name in &adapter_names {
        let Some(adapter) = find_adapter(asts, adapter_name) else {
            continue;
        };
        match &adapter.adapter_type {
            AdapterType::Http(http) => {
                for route in &http.routes {
                    use_case_names.insert(route.use_case.clone());
                    if let Some(infra) = route.auth.as_ref().and_then(|a| a.permission_infra()) {
                        infra_names.insert(infra.to_string());
                    }
                }
            }
            AdapterType::Grpc(grpc) => {
                for rpc in &grpc.rpcs {
                    use_case_names.insert(rpc.use_case.clone());
                }
            }
        }
    }

    let mut type_names: HashSet<String> = HashSet::new();

    for use_case_name in &use_case_names {
        let Some(use_case) = find_use_case(asts, use_case_name) else {
            continue;
        };
        for call in &use_case.calls {
            infra_names.insert(call.target.clone());
        }
        if let Some(ai) = &use_case.ai {
            if let Some(infra_name) = &ai.infra {
                infra_names.insert(infra_name.clone());
            }
        }
        if let Some(blob) = &use_case.blob {
            infra_names.insert(blob.target.clone());
        }
        if let Some(code) = &use_case.code {
            infra_names.extend(code.infras.iter().cloned());
            // A hand-written body may name any declared type, which this
            // closure can't see into — keep every type for this app.
            for ast in asts {
                if let Ast::TypeDecl(t) = ast {
                    type_names.insert(type_decl_name(t).to_string());
                }
            }
        }
        if let Some(input) = &use_case.input {
            collect_defined_type_name(input, &mut type_names);
        }
        if let Some(output) = &use_case.output {
            collect_defined_type_name(output, &mut type_names);
        }
    }

    // Foreign keys need the referenced tables in the same app.
    let mut pending: Vec<String> = infra_names.iter().cloned().collect();
    while let Some(infra_name) = pending.pop() {
        if let Some(InfraService { service_type: Storage(Psql(psql)), .. }) = find_infra(asts, &infra_name) {
            for reference in &psql.references {
                if infra_names.insert(reference.infra.clone()) {
                    pending.push(reference.infra.clone());
                }
            }
        }
    }

    for infra_name in &infra_names {
        let Some(infra) = find_infra(asts, infra_name) else {
            continue;
        };
        match &infra.service_type {
            Storage(Psql(psql)) => collect_defined_type_name(&psql.entity, &mut type_names),
            // Deliberately does NOT add `cfg.service` (the target adapter's
            // name) to `adapter_names` above — the target adapter belongs
            // to its own app and must never be generated inside this one.
            // It's resolved separately, read-only, against the full
            // unfiltered ast list at codegen time.
            Storage(StorageType::GrpcClient(cfg)) => {
                collect_defined_type_name(&cfg.entity, &mut type_names)
            }
            Storage(StorageType::S3(cfg)) => {
                collect_defined_type_name(&cfg.entity, &mut type_names)
            }
            InfraServiceType::HttpClient(_) => {}
        }
    }

    // Recursively close over struct field / type-alias references, since a
    // declared type can itself reference other declared types.
    let mut worklist: Vec<String> = type_names.iter().cloned().collect();
    while let Some(name) = worklist.pop() {
        let Some(decl) = find_type(asts, &name) else {
            continue;
        };
        let referenced_types: Vec<&AllowedType> = match decl {
            TypeDeclaration::Struct(s) => s.fields.iter().map(|f| &f.field_type).collect(),
            TypeDeclaration::TypeAlias(a) => vec![&a.aliased_type],
            TypeDeclaration::Enum(_) => vec![],
        };
        for allowed in referenced_types {
            let mut referenced = HashSet::new();
            collect_defined_type_name(allowed, &mut referenced);
            for referenced in referenced {
                if type_names.insert(referenced.clone()) {
                    worklist.push(referenced);
                }
            }
        }
    }

    asts.iter()
        .filter(|ast| match ast {
            Ast::Import(_) | Ast::Plugin(_) | Ast::Config(_) => true,
            Ast::App(a) => a.name == app.name,
            Ast::Adapter(a) => adapter_names.contains(&a.name),
            Ast::UseCase(u) => use_case_names.contains(&u.name),
            Ast::Infra(i) => infra_names.contains(&i.name),
            Ast::TypeDecl(t) => type_names.contains(type_decl_name(t)),
            Ast::Schedule(s) => schedule_names.contains(&s.name),
        })
        .cloned()
        .collect()
}

fn collect_defined_type_name(allowed: &AllowedType, out: &mut HashSet<String>) {
    match allowed {
        AllowedType::DefinedType(name) => {
            out.insert(name.clone());
        }
        AllowedType::Option(inner) | AllowedType::List(inner) => collect_defined_type_name(inner, out),
        _ => {}
    }
}

fn type_decl_name(t: &TypeDeclaration) -> &str {
    t.name()
}

fn find_adapter<'a>(asts: &'a [Ast], name: &str) -> Option<&'a AdapterService> {
    asts.iter().find_map(|ast| match ast {
        Ast::Adapter(a) if a.name == name => Some(a),
        _ => None,
    })
}

fn find_schedule<'a>(asts: &'a [Ast], name: &str) -> Option<&'a crate::parser::ScheduleDeclaration> {
    asts.iter().find_map(|ast| match ast {
        Ast::Schedule(s) if s.name == name => Some(s),
        _ => None,
    })
}

fn find_use_case<'a>(asts: &'a [Ast], name: &str) -> Option<&'a UseCaseService> {
    asts.iter().find_map(|ast| match ast {
        Ast::UseCase(u) if u.name == name => Some(u),
        _ => None,
    })
}

fn find_infra<'a>(asts: &'a [Ast], name: &str) -> Option<&'a InfraService> {
    asts.iter().find_map(|ast| match ast {
        Ast::Infra(i) if i.name == name => Some(i),
        _ => None,
    })
}

fn find_type<'a>(asts: &'a [Ast], name: &str) -> Option<&'a TypeDeclaration> {
    asts.iter().find_map(|ast| match ast {
        Ast::TypeDecl(t) if type_decl_name(t) == name => Some(t),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{
        AdapterType, AllowedType, GrpcAdapterService, GrpcRpc, HttpAdapterService, HttpMethod,
        HttpRoute, InfraServiceType, PsqlInfraService, StorageType, StructDeclaration,
        StructField, UseCaseCall,
    };

    fn type_decl(name: &str, fields: Vec<(&str, AllowedType)>) -> Ast {
        Ast::TypeDecl(TypeDeclaration::Struct(StructDeclaration {
            name: name.to_string(),
            fields: fields
                .into_iter()
                .map(|(n, t)| StructField { name: n.to_string(), field_type: t })
                .collect(),
            export: None,
        }))
    }

    fn http_adapter(name: &str, use_case: &str) -> Ast {
        Ast::Adapter(AdapterService {
            name: name.to_string(),
            adapter_type: AdapterType::Http(HttpAdapterService {
                on: Some("0.0.0.0:8080".to_string()),
                cors: None,
                routes: vec![HttpRoute {
                    method: HttpMethod::Get,
                    path: "/x".to_string(),
                    input: None,
                    output: None,
                    use_case: use_case.to_string(),
                    status: vec![],
                    auth: None,
                }],
            }),
        })
    }

    fn grpc_adapter(name: &str, use_case: &str) -> Ast {
        Ast::Adapter(AdapterService {
            name: name.to_string(),
            adapter_type: AdapterType::Grpc(GrpcAdapterService {
                on: Some("0.0.0.0:50051".to_string()),
                rpcs: vec![GrpcRpc {
                    name: "Do".to_string(),
                    input: None,
                    output: None,
                    use_case: use_case.to_string(),
                }],
            }),
        })
    }

    fn use_case(name: &str, output: AllowedType, target: &str) -> Ast {
        Ast::UseCase(UseCaseService {
            name: name.to_string(),
            input: None,
            output: Some(output),
            calls: vec![UseCaseCall {
                target: target.to_string(),
                method: "find_one".to_string(),
                errors: vec![],
            }],
            ai: None,
            blob: None,
            filters: vec![],
            owner: None,
            code: None,
            order_by: vec![],
            transaction: false,
            notify: None,
        })
    }

    fn psql_infra(name: &str, entity: &str) -> Ast {
        Ast::Infra(InfraService {
            name: name.to_string(),
            service_type: InfraServiceType::Storage(StorageType::Psql(PsqlInfraService {
                entity: AllowedType::DefinedType(entity.to_string()),
                ..Default::default()
            })),
        })
    }

    fn grpc_client_infra(name: &str, service: &str, entity: &str) -> Ast {
        Ast::Infra(InfraService {
            name: name.to_string(),
            service_type: InfraServiceType::Storage(StorageType::GrpcClient(
                crate::parser::GrpcClientInfraService {
                    service: service.to_string(),
                    on: "127.0.0.1:50051".to_string(),
                    entity: AllowedType::DefinedType(entity.to_string()),
                    methods: vec![crate::parser::GrpcClientMethodMapping {
                        storage_method: "find_one".to_string(),
                        rpc: "GetTask".to_string(),
                    }],
                },
            )),
        })
    }

    fn app(name: &str, adapters: Vec<&str>) -> Ast {
        Ast::App(AppService {
            name: name.to_string(),
            adapters: adapters.into_iter().map(String::from).collect(),
            load_balancers: vec![],
        })
    }

    #[test]
    fn partitions_two_apps_into_disjoint_reachable_subsets() {
        let asts = vec![
            type_decl("Task", vec![("id", AllowedType::Int)]),
            type_decl("Doc", vec![("id", AllowedType::Int)]),
            psql_infra("TaskInfra", "Task"),
            psql_infra("DocInfra", "Doc"),
            use_case("GetTaskUseCase", AllowedType::DefinedType("Task".to_string()), "TaskInfra"),
            use_case("GetDocUseCase", AllowedType::DefinedType("Doc".to_string()), "DocInfra"),
            http_adapter("HttpApiAdapter", "GetTaskUseCase"),
            grpc_adapter("GrpcWorkerAdapter", "GetDocUseCase"),
            app("HttpApi", vec!["HttpApiAdapter"]),
            app("GrpcWorker", vec!["GrpcWorkerAdapter"]),
        ];

        let partitioned = partition_by_app(&asts).expect("should partition");
        assert_eq!(partitioned.len(), 2);

        let (http_app, http_asts) = &partitioned[0];
        assert_eq!(http_app.name, "HttpApi");
        assert!(http_asts.iter().any(|a| matches!(a, Ast::Adapter(x) if x.name == "HttpApiAdapter")));
        assert!(!http_asts.iter().any(|a| matches!(a, Ast::Adapter(x) if x.name == "GrpcWorkerAdapter")));
        assert!(http_asts.iter().any(|a| matches!(a, Ast::UseCase(x) if x.name == "GetTaskUseCase")));
        assert!(!http_asts.iter().any(|a| matches!(a, Ast::UseCase(x) if x.name == "GetDocUseCase")));
        assert!(http_asts.iter().any(|a| matches!(a, Ast::Infra(x) if x.name == "TaskInfra")));
        assert!(!http_asts.iter().any(|a| matches!(a, Ast::Infra(x) if x.name == "DocInfra")));
        assert!(http_asts.iter().any(|a| matches!(a, Ast::TypeDecl(TypeDeclaration::Struct(s)) if s.name == "Task")));
        assert!(!http_asts.iter().any(|a| matches!(a, Ast::TypeDecl(TypeDeclaration::Struct(s)) if s.name == "Doc")));

        let (grpc_app, grpc_asts) = &partitioned[1];
        assert_eq!(grpc_app.name, "GrpcWorker");
        assert!(grpc_asts.iter().any(|a| matches!(a, Ast::Adapter(x) if x.name == "GrpcWorkerAdapter")));
        assert!(!grpc_asts.iter().any(|a| matches!(a, Ast::Adapter(x) if x.name == "HttpApiAdapter")));
    }

    #[test]
    fn recurses_through_nested_defined_type_fields() {
        let asts = vec![
            type_decl("Address", vec![("city", AllowedType::String)]),
            type_decl("User", vec![("address", AllowedType::DefinedType("Address".to_string()))]),
            psql_infra("UserInfra", "User"),
            use_case("GetUserUseCase", AllowedType::DefinedType("User".to_string()), "UserInfra"),
            http_adapter("Adapter", "GetUserUseCase"),
            app("Main", vec!["Adapter"]),
        ];

        let partitioned = partition_by_app(&asts).expect("should partition");
        let (_, main_asts) = &partitioned[0];
        assert!(main_asts.iter().any(|a| matches!(a, Ast::TypeDecl(TypeDeclaration::Struct(s)) if s.name == "User")));
        assert!(main_asts.iter().any(|a| matches!(a, Ast::TypeDecl(TypeDeclaration::Struct(s)) if s.name == "Address")));
    }

    #[test]
    fn grpc_client_infra_pulls_in_its_entity_but_not_the_target_adapter() {
        let asts = vec![
            type_decl("Task", vec![("id", AllowedType::Int)]),
            grpc_client_infra("RemoteTaskInfra", "GrpcWorkerAdapter", "Task"),
            use_case("GetTaskUseCase", AllowedType::DefinedType("Task".to_string()), "RemoteTaskInfra"),
            http_adapter("HttpApiAdapter", "GetTaskUseCase"),
            // Declared in the file (as it would be, in a different app) but
            // must never leak into HttpApi's own crate.
            grpc_adapter("GrpcWorkerAdapter", "SomeOtherUseCase"),
            app("HttpApi", vec!["HttpApiAdapter"]),
        ];

        let partitioned = partition_by_app(&asts).expect("should partition");
        let (http_app, http_asts) = &partitioned[0];
        assert_eq!(http_app.name, "HttpApi");
        assert!(http_asts.iter().any(|a| matches!(a, Ast::Infra(x) if x.name == "RemoteTaskInfra")));
        assert!(http_asts.iter().any(|a| matches!(a, Ast::TypeDecl(TypeDeclaration::Struct(s)) if s.name == "Task")));
        assert!(!http_asts.iter().any(|a| matches!(a, Ast::Adapter(x) if x.name == "GrpcWorkerAdapter")));
    }

    #[test]
    fn duplicate_app_names_are_rejected() {
        let asts = vec![app("Main", vec![]), app("Main", vec![])];
        let err = partition_by_app(&asts).unwrap_err();
        assert_eq!(err, "Duplicate app name 'Main'");
    }
}
