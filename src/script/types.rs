//! The types a script manipulates, and how the project's declarations map
//! onto them.

use std::collections::HashMap;

use crate::parser::{
    AllowedType, Ast, DerivedType, InfraServiceType, StorageType, TypeDeclaration,
};

#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    Int,
    Float,
    Decimal,
    Bool,
    Str,
    Uuid,
    DateTime,
    Date,
    Time,
    Json,
    Opt(Box<Ty>),
    List(Box<Ty>),
    /// A declared struct (or `Omit`/`Pick`/`Partial` type).
    Struct(String),
    /// A declared enum of plain values.
    Enum(String),
    /// A declared enum whose variants carry fields: passed around only.
    Tagged(String),
    /// The built-in `RawRequest` input.
    RawRequest,
    /// `try e`: the value, or the error's text (`Result<T, String>`).
    Try(Box<Ty>),
    /// `Api.send(...)`: status and body of an HTTP response, whatever the
    /// status (`(i64, String)`).
    Response,
    /// `Map<K, V>`: keys in order (`BTreeMap`).
    Map(Box<Ty>, Box<Ty>),
    /// What `xs.push(x)` returns.
    Unit,
    /// What `fail` "returns": fits anywhere.
    Never,
}

impl Ty {
    pub fn opt(inner: Ty) -> Ty {
        Ty::Opt(Box::new(inner))
    }

    pub fn list(inner: Ty) -> Ty {
        Ty::List(Box::new(inner))
    }

    pub fn name(&self) -> String {
        match self {
            Ty::Int => "Int".into(),
            Ty::Float => "Float".into(),
            Ty::Decimal => "Decimal".into(),
            Ty::Bool => "Bool".into(),
            Ty::Str => "String".into(),
            Ty::Uuid => "Uuid".into(),
            Ty::DateTime => "DateTime".into(),
            Ty::Date => "Date".into(),
            Ty::Time => "Time".into(),
            Ty::Json => "Json".into(),
            Ty::Opt(t) => format!("Option<{}>", t.name()),
            Ty::List(t) => format!("List<{}>", t.name()),
            Ty::Struct(n) | Ty::Enum(n) | Ty::Tagged(n) => n.clone(),
            Ty::RawRequest => "RawRequest".into(),
            Ty::Try(t) => format!("Try<{}>", t.name()),
            Ty::Map(k, v) => format!("Map<{}, {}>", k.name(), v.name()),
            Ty::Response => "Response".into(),
            Ty::Unit => "nothing".into(),
            Ty::Never => "fail".into(),
        }
    }

    /// The type as the generated crate spells it.
    pub fn rust(&self) -> String {
        match self {
            Ty::Int => "i64".into(),
            Ty::Float => "f64".into(),
            Ty::Decimal => "rust_decimal::Decimal".into(),
            Ty::Bool => "bool".into(),
            Ty::Str => "String".into(),
            Ty::Uuid => "uuid::Uuid".into(),
            Ty::DateTime => "chrono::DateTime<chrono::Utc>".into(),
            Ty::Date => "chrono::NaiveDate".into(),
            Ty::Time => "chrono::NaiveTime".into(),
            Ty::Json => "serde_json::Value".into(),
            Ty::Opt(t) => format!("Option<{}>", t.rust()),
            Ty::List(t) => format!("Vec<{}>", t.rust()),
            Ty::Struct(n) | Ty::Enum(n) | Ty::Tagged(n) => n.clone(),
            Ty::RawRequest => "RawRequest".into(),
            Ty::Try(t) => format!("Result<{}, String>", t.rust()),
            Ty::Map(k, v) => format!("std::collections::BTreeMap<{}, {}>", k.rust(), v.rust()),
            Ty::Response => "(i64, String)".into(),
            Ty::Unit => "()".into(),
            Ty::Never => "_".into(),
        }
    }

    pub fn is_copy(&self) -> bool {
        match self {
            Ty::Int
            | Ty::Float
            | Ty::Decimal
            | Ty::Bool
            | Ty::Uuid
            | Ty::DateTime
            | Ty::Date
            | Ty::Time
            | Ty::Enum(_) => true,
            Ty::Opt(t) => t.is_copy(),
            _ => false,
        }
    }

    /// Supports `==` (generated structs don't derive `PartialEq`).
    pub fn is_eq(&self) -> bool {
        match self {
            Ty::Struct(_)
            | Ty::Tagged(_)
            | Ty::RawRequest
            | Ty::Unit
            | Ty::Never
            | Ty::Try(_)
            | Ty::Response
            | Ty::Map(..) => false,
            Ty::Opt(t) | Ty::List(t) => t.is_eq(),
            _ => true,
        }
    }

    /// Supports `<` and sorting.
    pub fn is_ord(&self) -> bool {
        match self {
            Ty::Int | Ty::Float | Ty::Decimal | Ty::Str | Ty::DateTime | Ty::Date | Ty::Time => {
                true
            }
            Ty::Opt(t) => t.is_ord(),
            _ => false,
        }
    }

    /// Totally ordered (`max()`/`min()` of a list; not `Float`).
    pub fn is_total(&self) -> bool {
        self.is_ord() && !matches!(self, Ty::Float) && !matches!(self, Ty::Opt(t) if !t.is_total())
    }

    /// Can be written into a string (`"{x}"`) or a query.
    pub fn is_display(&self) -> bool {
        matches!(
            self,
            Ty::Int
                | Ty::Float
                | Ty::Decimal
                | Ty::Bool
                | Ty::Str
                | Ty::Uuid
                | Ty::DateTime
                | Ty::Date
                | Ty::Time
                | Ty::Enum(_)
                | Ty::Json
        )
    }

    pub fn is_numeric(&self) -> bool {
        matches!(self, Ty::Int | Ty::Float | Ty::Decimal)
    }
}

/// An infra, as far as a script can use it.
#[derive(Debug, Clone)]
pub enum InfraKind {
    Psql {
        entity: String,
        owner: Option<String>,
    },
    /// `ics`: an `ics_feed {}` (also has `occurrences(from, to)`);
    /// `notify`: a notifications router (`send`), recording into `record`.
    HttpClient {
        ics: bool,
        notify: bool,
        record: Option<String>,
    },
    /// Any other kind: named in the error message.
    Other(&'static str),
}

/// What a script can see of the project.
#[derive(Debug, Default)]
pub struct Project {
    pub types: HashMap<String, TypeDeclaration>,
    pub infras: HashMap<String, InfraKind>,
    /// `(input, output)` of every use case.
    pub use_cases: HashMap<String, (Option<AllowedType>, Option<AllowedType>)>,
    /// `map A -> B` declarations: `(A, B)`.
    pub maps: Vec<(String, String)>,
    /// A `config { tz | locale }` exists: the crate has `crate::tz`.
    pub local_time: bool,
    /// A `config { locale }` exists: the crate has `crate::fmt`.
    pub locale: bool,
    /// `extern fn` declarations of top-level `logic {}` blocks.
    pub externs: HashMap<String, Extern>,
}

/// A Rust function scripts may call (see `ExternFn`), resolved.
#[derive(Debug, Clone)]
pub struct Extern {
    /// `(type, passed by reference)`.
    pub params: Vec<(Ty, bool)>,
    pub ret: Ty,
    /// The Rust file, resolved: included in the crate when used.
    pub file: String,
}

impl Project {
    /// The owner field of every entity stored in an `owner:` psql infra:
    /// `create` / `update` fill it, so struct literals may leave it out.
    pub fn is_owner_field(&self, entity: &str, field: &str) -> bool {
        self.infras.values().any(|k| {
            matches!(k, InfraKind::Psql { entity: e, owner: Some(o) } if e == entity && o == field)
        })
    }

    pub fn new(asts: &[Ast]) -> Self {
        let mut project = Project::default();
        for ast in asts {
            match ast {
                Ast::TypeDecl(t) => {
                    let name = match t {
                        TypeDeclaration::TypeAlias(a) => a.name.clone(),
                        TypeDeclaration::Struct(s) => s.name.clone(),
                        TypeDeclaration::Enum(e) => e.name.clone(),
                        TypeDeclaration::Derived(d) => d.name.clone(),
                    };
                    project.types.insert(name, t.clone());
                }
                Ast::Infra(infra) => {
                    let kind = match &infra.service_type {
                        InfraServiceType::Storage(StorageType::Psql(psql)) => match &psql.entity {
                            AllowedType::DefinedType(entity) => InfraKind::Psql {
                                entity: entity.clone(),
                                owner: psql.owner.clone(),
                            },
                            _ => InfraKind::Other("psql"),
                        },
                        InfraServiceType::HttpClient(c) => {
                            if c.ics.is_some() && !project.types.contains_key("IcsOccurrence") {
                                project
                                    .types
                                    .insert("IcsOccurrence".to_string(), ics_occurrence());
                            }
                            InfraKind::HttpClient {
                                ics: c.ics.is_some(),
                                notify: c.notify.is_some(),
                                record: c.notify.as_ref().and_then(|n| n.record.clone()),
                            }
                        }
                        InfraServiceType::Storage(StorageType::S3(_)) => InfraKind::Other("s3"),
                        InfraServiceType::Storage(StorageType::GrpcClient(_)) => {
                            InfraKind::Other("grpc_client")
                        }
                    };
                    project.infras.insert(infra.name.clone(), kind);
                }
                Ast::UseCase(u) => {
                    project
                        .use_cases
                        .insert(u.name.clone(), (u.input.clone(), u.output.clone()));
                }
                Ast::Map(m) => project.maps.push((m.from.clone(), m.to.clone())),
                Ast::Config(c) => {
                    project.local_time |= c.tz.is_some() || c.locale.is_some();
                    project.locale |= c.locale.is_some();
                }
                _ => {}
            }
        }
        project
    }

    pub fn ty(&self, t: &AllowedType) -> Result<Ty, String> {
        Ok(match t {
            AllowedType::String | AllowedType::Regex(_) => Ty::Str,
            AllowedType::Uuid => Ty::Uuid,
            AllowedType::Int => Ty::Int,
            AllowedType::Float => Ty::Float,
            AllowedType::Decimal => Ty::Decimal,
            AllowedType::Bool => Ty::Bool,
            AllowedType::DateTime => Ty::DateTime,
            AllowedType::Date => Ty::Date,
            AllowedType::Time => Ty::Time,
            AllowedType::Json => Ty::Json,
            AllowedType::Option(inner) => Ty::opt(self.ty(inner)?),
            AllowedType::List(inner) => Ty::list(self.ty(inner)?),
            AllowedType::Range(inner, _, _) => self.ty(inner)?,
            AllowedType::DefinedType(name) => self.named(name)?,
        })
    }

    pub fn named(&self, name: &str) -> Result<Ty, String> {
        match self.types.get(name) {
            Some(TypeDeclaration::Struct(_)) | Some(TypeDeclaration::Derived(_)) => {
                Ok(Ty::Struct(name.to_string()))
            }
            Some(TypeDeclaration::Enum(e)) if e.is_tagged() => Ok(Ty::Tagged(name.to_string())),
            Some(TypeDeclaration::Enum(_)) => Ok(Ty::Enum(name.to_string())),
            Some(TypeDeclaration::TypeAlias(a)) => self.ty(&a.aliased_type),
            None if name == "RawRequest" => Ok(Ty::RawRequest),
            None => Err(format!(
                "unknown type '{}' (logic only sees types declared in this project, not imported ones)",
                name
            )),
        }
    }

    /// The fields of a struct type, in declaration order.
    pub fn fields(&self, name: &str) -> Result<Vec<(String, AllowedType)>, String> {
        match self.types.get(name) {
            Some(TypeDeclaration::Struct(s)) => Ok(s
                .fields
                .iter()
                .map(|f| (f.name.clone(), f.field_type.clone()))
                .collect()),
            Some(TypeDeclaration::Derived(d)) => {
                let mut fields = self.derived_fields(&d.base)?;
                fields.extend(
                    d.extra
                        .iter()
                        .map(|f| (f.name.clone(), f.field_type.clone())),
                );
                Ok(fields)
            }
            _ => Err(format!("'{}' is not a struct", name)),
        }
    }

    fn derived_fields(&self, derived: &DerivedType) -> Result<Vec<(String, AllowedType)>, String> {
        Ok(match derived {
            DerivedType::Named(name) => self.fields(name)?,
            DerivedType::Omit(base, names) => self
                .derived_fields(base)?
                .into_iter()
                .filter(|(f, _)| !names.contains(f))
                .collect(),
            DerivedType::Pick(base, names) => self
                .derived_fields(base)?
                .into_iter()
                .filter(|(f, _)| names.contains(f))
                .collect(),
            DerivedType::Partial(base) => self
                .derived_fields(base)?
                .into_iter()
                .map(|(f, t)| match t {
                    AllowedType::Option(_) => (f, t),
                    t => (f, AllowedType::Option(Box::new(t))),
                })
                .collect(),
        })
    }

    /// The fields of `variant` of the tagged enum `name`, if it has it.
    pub fn variant_fields(&self, name: &str, variant: &str) -> Option<Vec<(String, AllowedType)>> {
        match self.types.get(name) {
            Some(TypeDeclaration::Enum(e)) => {
                e.variants.iter().position(|v| v == variant).map(|i| {
                    e.fields
                        .get(i)
                        .map(|f| {
                            f.iter()
                                .map(|f| (f.name.clone(), f.field_type.clone()))
                                .collect()
                        })
                        .unwrap_or_default()
                })
            }
            _ => None,
        }
    }

    pub fn enum_variants(&self, name: &str) -> Vec<String> {
        match self.types.get(name) {
            Some(TypeDeclaration::Enum(e)) => e.variants.clone(),
            _ => Vec::new(),
        }
    }
}

/// `crate::IcsOccurrence`, generated with `ics_feed {}` infras.
fn ics_occurrence() -> TypeDeclaration {
    use crate::parser::{StructDeclaration, StructField};
    let field = |name: &str, field_type: AllowedType| StructField {
        name: name.to_string(),
        field_type,
    };
    let text = || AllowedType::Option(Box::new(AllowedType::String));
    TypeDeclaration::Struct(StructDeclaration {
        name: "IcsOccurrence".to_string(),
        fields: vec![
            field("uid", AllowedType::String),
            field("instance_id", AllowedType::String),
            field("title", AllowedType::String),
            field("start", AllowedType::DateTime),
            field("end", AllowedType::DateTime),
            field("location", text()),
            field("description", text()),
            field("all_day", AllowedType::Bool),
        ],
        export: None,
    })
}
