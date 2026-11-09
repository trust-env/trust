
use crate::generator::rust_code::{
    _trait::{Trait, TraitImpl},
    RustCode,
    include::Include,
    render::render,
    service::{Attribute, Method, Service},
    types::{RustField, RustType},
};
use serde::Serialize;
use tera::Context;

#[derive(Serialize)]
struct DataFieldView {
    name: String,
}

/// One column with its Postgres type, as written by the migration.
#[derive(Serialize, Clone)]
struct ColumnView {
    name: String,
    sql_type: String,
}

pub struct PsqlStorageService {
    pub entity: RustType,
    error_name: String,
    storage_trait_name: String,
    columns: Vec<ColumnView>,
}

impl PsqlStorageService {
    pub fn new(entity: RustType, storage_trait: Trait) -> Result<Self, String> {
        let error_name = format!("Psql{}StorageError", entity.name);
        let mut columns = Vec::new();
        for field in &entity.fields {
            let (sql_type, _) = crate::generator::rust_code::migration::sql_type(
                &entity.name,
                &field.name,
                &field.field_type,
            )?;
            columns.push(ColumnView {
                name: field.name.clone(),
                sql_type,
            });
        }

        Ok(PsqlStorageService {
            entity,
            error_name,
            storage_trait_name: storage_trait.name,
            columns,
        })
    }

    fn has_id(&self) -> bool {
        self.entity.fields.iter().any(|f| f.name == "id")
    }

    fn column_type(&self, name: &str) -> &str {
        self.columns
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.sql_type.as_str())
            .unwrap_or("TEXT")
    }

    /// Integer, non-key columns — the only ones `owner:` may name (the
    /// use_case codegen validates `owner` is an `Int` field).
    fn owner_columns(&self) -> Vec<String> {
        self.entity
            .fields
            .iter()
            .filter(|f| f.name != "id" && f.field_type.name == "i64")
            .map(|f| f.name.clone())
            .collect()
    }

    pub fn error_name(&self) -> &str {
        &self.error_name
    }

    fn table_name(&self) -> String {
        crate::generator::rust_code::migration::table_name(&self.entity.name)
    }

    fn id_field(&self) -> String {
        self.entity
            .fields
            .iter()
            .find(|f| f.name == "id")
            .map(|f| f.name.clone())
            .unwrap_or_else(|| "id".to_string())
    }

    fn data_fields(&self) -> Vec<&RustField> {
        self.entity.fields.iter().filter(|f| f.name != "id").collect()
    }

    fn all_columns(&self) -> String {
        self.entity
            .fields
            .iter()
            .map(|f| f.name.clone())
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn method(
        &self,
        name: &str,
        parameters: Vec<(&str, &str)>,
        return_type: &str,
        content: String,
    ) -> Method {
        Method {
            name: name.to_string(),
            is_pub: false,
            is_async: true,
            templates: vec![],
            parameters: parameters
                .into_iter()
                .map(|(param_name, param_type)| {
                    (param_name.to_string(), RustType::primitive(param_type))
                })
                .collect(),
            return_type: RustType::primitive(return_type),
            content,
        }
    }

    fn data_field_views(&self) -> Vec<DataFieldView> {
        self.data_fields()
            .into_iter()
            .map(|f| DataFieldView { name: f.name.clone() })
            .collect()
    }

    fn save_one_method(&self) -> Method {
        let entity = self.entity.to_call_string(vec![]);
        let data = self.data_fields();
        let columns = data.iter().map(|f| f.name.clone()).collect::<Vec<_>>().join(", ");
        let placeholders = (1..=data.len())
            .map(|i| format!("${}", i))
            .collect::<Vec<_>>()
            .join(", ");

        let mut context = Context::new();
        context.insert("entity", &entity);
        context.insert("table", &self.table_name());
        context.insert("columns", &columns);
        context.insert("placeholders", &placeholders);
        context.insert("all_columns", &self.all_columns());
        context.insert("data_fields", &self.data_field_views());
        context.insert("error", &self.error_name);
        let content = render("service/psql_storage/save_one.rs.tera", &context);

        self.method(
            "save_one",
            vec![("data", &entity)],
            &format!("Result<{}, {}>", entity, self.error_name),
            content,
        )
    }

    /// A single `INSERT ... SELECT * FROM UNNEST($1::T1[], $2::T2[], ...)`
    /// instead of one `save_one` round-trip per row.
    fn save_many_method(&self) -> Method {
        let entity = self.entity.to_call_string(vec![]);
        let data = self.data_fields();
        let columns = data.iter().map(|f| f.name.clone()).collect::<Vec<_>>().join(", ");
        let unnest_args = data
            .iter()
            .enumerate()
            .map(|(i, f)| format!("${}::{}[]", i + 1, self.column_type(&f.name)))
            .collect::<Vec<_>>()
            .join(", ");

        let mut context = Context::new();
        context.insert("entity", &entity);
        context.insert("table", &self.table_name());
        context.insert("columns", &columns);
        context.insert("unnest_args", &unnest_args);
        context.insert("all_columns", &self.all_columns());
        context.insert("data_fields", &self.data_field_views());
        context.insert("error", &self.error_name);
        let content = render("service/psql_storage/save_many.rs.tera", &context);

        self.method(
            "save_many",
            vec![("data", &format!("Vec<{}>", entity))],
            &format!("Result<Vec<{}>, {}>", entity, self.error_name),
            content,
        )
    }

    fn find_one_method(&self) -> Method {
        let entity = self.entity.to_call_string(vec![]);

        let mut context = Context::new();
        context.insert("entity", &entity);
        context.insert("table", &self.table_name());
        context.insert("all_columns", &self.all_columns());
        context.insert("id_field", &self.id_field());
        context.insert("error", &self.error_name);
        let content = render("service/psql_storage/find_one.rs.tera", &context);

        self.method(
            "find_one",
            vec![("id", "EntityId")],
            &format!("Result<Option<{}>, {}>", entity, self.error_name),
            content,
        )
    }

    fn find_many_method(&self) -> Method {
        let entity = self.entity.to_call_string(vec![]);

        let mut context = Context::new();
        context.insert("entity", &entity);
        context.insert("table", &self.table_name());
        context.insert("all_columns", &self.all_columns());
        context.insert("id_field", &self.id_field());
        context.insert("error", &self.error_name);
        let content = render("service/psql_storage/find_many.rs.tera", &context);

        self.method(
            "find_many",
            vec![("ids", "Vec<EntityId>")],
            &format!("Result<Vec<{}>, {}>", entity, self.error_name),
            content,
        )
    }

    /// `query.predicates` is a runtime-length list, so the WHERE clause and
    /// its binds are built dynamically (unlike the other methods, whose
    /// placeholder count is known at generation time).
    /// `StoragePredicate::value` is always a `String`, so each *parameter*
    /// is cast to its column's type (`col = $1::BIGINT`) — never the column
    /// itself (`col::text = $1`), which would defeat every index on it.
    /// Field names are checked against the entity's columns before being
    /// spliced into the SQL. Rows come back ordered by `id` (when the entity
    /// has one) so `LIMIT`/`OFFSET` pages are stable.
    fn list_method(&self) -> Method {
        let entity = self.entity.to_call_string(vec![]);

        let mut context = Context::new();
        context.insert("entity", &entity);
        context.insert("table", &self.table_name());
        context.insert("all_columns", &self.all_columns());
        context.insert("columns", &self.columns);
        context.insert("order_by", &if self.has_id() { self.id_field() } else { String::new() });
        context.insert("error", &self.error_name);
        let content = render("service/psql_storage/list.rs.tera", &context);

        self.method(
            "list",
            vec![("query", "StorageListQuery")],
            &format!("Result<Vec<{}>, {}>", entity, self.error_name),
            content,
        )
    }

    fn update_one_method(&self) -> Method {
        let entity = self.entity.to_call_string(vec![]);
        let data = self.data_fields();
        let set_clause = data
            .iter()
            .enumerate()
            .map(|(i, f)| format!("{} = ${}", f.name, i + 1))
            .collect::<Vec<_>>()
            .join(", ");

        let mut context = Context::new();
        context.insert("entity", &entity);
        context.insert("table", &self.table_name());
        context.insert("set_clause", &set_clause);
        context.insert("id_field", &self.id_field());
        context.insert("next", &(data.len() + 1));
        context.insert("all_columns", &self.all_columns());
        context.insert("data_fields", &self.data_field_views());
        context.insert("error", &self.error_name);
        let content = render("service/psql_storage/update_one.rs.tera", &context);

        self.method(
            "update_one",
            vec![("id", "EntityId"), ("data", &entity)],
            &format!("Result<{}, {}>", entity, self.error_name),
            content,
        )
    }

    fn delete_method(&self) -> Method {
        let entity = self.entity.to_call_string(vec![]);

        let mut context = Context::new();
        context.insert("entity", &entity);
        context.insert("table", &self.table_name());
        context.insert("all_columns", &self.all_columns());
        context.insert("id_field", &self.id_field());
        context.insert("error", &self.error_name);
        let content = render("service/psql_storage/delete.rs.tera", &context);

        self.method(
            "delete",
            vec![("id", "EntityId")],
            &format!("Result<{}, {}>", entity, self.error_name),
            content,
        )
    }

    fn delete_many_method(&self) -> Method {
        let entity = self.entity.to_call_string(vec![]);

        let mut context = Context::new();
        context.insert("entity", &entity);
        context.insert("table", &self.table_name());
        context.insert("all_columns", &self.all_columns());
        context.insert("id_field", &self.id_field());
        context.insert("error", &self.error_name);
        let content = render("service/psql_storage/delete_many.rs.tera", &context);

        self.method(
            "delete_many",
            vec![("ids", "Vec<EntityId>")],
            &format!("Result<Vec<{}>, {}>", entity, self.error_name),
            content,
        )
    }

    fn storage_methods(&self) -> Vec<Method> {
        vec![
            self.save_one_method(),
            self.save_many_method(),
            self.find_one_method(),
            self.find_many_method(),
            self.list_method(),
            self.update_one_method(),
            self.delete_method(),
            self.delete_many_method(),
        ]
    }

    fn storage_impl(&self) -> TraitImpl {
        TraitImpl {
            trait_name: format!("Storage<{}, {}>", self.entity.to_call_string(vec![]), self.error_name),
            methods: self.storage_methods(),
            templates: vec![],
            include: Some(Include {
                crate_link: "ssh://git@github.com/BH-World/common.git".to_string(),
                crate_name: "common".to_string(),
            }),
        }
    }
}

impl PsqlStorageService {
    fn service(&self) -> Service {
        Service {
            name: format!("Psql{}Storage", self.entity.name),
            templates: vec![],
            attrs: vec![Attribute::sql_connector()],
            traits_impls: vec![self.storage_impl()],
        }
    }

    /// `pub trait {Entity}Storage: Storage<...> + Send + Sync { ... }` —
    /// the bound every use case is generic over, extended with the
    /// single-statement owner-checked writes `owner:` use cases call.
    pub fn storage_trait_code(&self) -> String {
        let mut context = Context::new();
        context.insert("trait_name", &self.storage_trait_name);
        context.insert("entity", &self.entity.to_call_string(vec![]));
        context.insert("error", &self.error_name);
        render("_trait/owned_storage_trait.rs.tera", &context)
    }

    fn owned_impl_code(&self) -> String {
        let data = self.data_fields();
        let set_clause = data
            .iter()
            .enumerate()
            .map(|(i, f)| format!("{} = ${}", f.name, i + 1))
            .collect::<Vec<_>>()
            .join(", ");

        let mut context = Context::new();
        context.insert("trait_name", &self.storage_trait_name);
        context.insert("struct_name", &format!("Psql{}Storage", self.entity.name));
        context.insert("entity", &self.entity.to_call_string(vec![]));
        context.insert("table", &self.table_name());
        context.insert("set_clause", &set_clause);
        context.insert("id_field", &self.id_field());
        context.insert("next", &(data.len() + 1));
        context.insert("next_owner", &(data.len() + 2));
        context.insert("all_columns", &self.all_columns());
        context.insert("data_fields", &self.data_field_views());
        context.insert("owner_columns", &self.owner_columns());
        context.insert("error", &self.error_name);
        render("service/psql_storage/owned.rs.tera", &context)
    }

    /// Just the `pub struct Psql{Entity}Storage { ... }` definition, with no
    /// trailing impls, so the caller can place attributes (e.g. `#[derive(Clone)]`)
    /// directly above it.
    pub fn struct_code(&self) -> String {
        let service = Service {
            traits_impls: vec![],
            ..self.service()
        };
        service.to_rust_code()
    }

    pub fn ctor_code(&self) -> String {
        format!(
            "impl Psql{entity}Storage {{\n    pub fn new(db: SQLConnector) -> Self {{\n        Self {{ db }}\n    }}\n}}\n\n",
            entity = self.entity.name,
        )
    }

    pub fn trait_impls_code(&self) -> String {
        let service = self.service();
        let mut code: String = service
            .traits_impls
            .iter()
            .map(|t| t.to_rust_code(&service) + "\n")
            .collect();
        code.push_str(&self.owned_impl_code());
        code
    }
}
