//! Hand-written axum + sqlx baseline: the same stack Trust generates, minus
//! the generated layers (use-case traits, error mapping, tracing middleware).
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, postgres::PgPoolOptions};

#[derive(Serialize, FromRow)]
struct User {
    id: i64,
    name: String,
    email: String,
    age: i64,
}

#[derive(Deserialize)]
struct NewUser {
    name: String,
    email: String,
    age: i64,
}

#[derive(Deserialize)]
struct Page {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Serialize)]
struct One {
    user: User,
}

#[derive(Serialize)]
struct Many {
    users: Vec<User>,
}

#[derive(Serialize)]
struct Pong {
    ok: bool,
}

type ApiResult<T> = Result<Json<T>, StatusCode>;

fn internal(_: sqlx::Error) -> StatusCode {
    StatusCode::INTERNAL_SERVER_ERROR
}

async fn ping() -> Json<Pong> {
    Json(Pong { ok: true })
}

async fn get_user(State(db): State<PgPool>, Path(id): Path<i64>) -> ApiResult<One> {
    sqlx::query_as::<_, User>("SELECT id, name, email, age FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(&db)
        .await
        .map_err(internal)?
        .map(|user| Json(One { user }))
        .ok_or(StatusCode::NOT_FOUND)
}

async fn list_users(State(db): State<PgPool>, Query(page): Query<Page>) -> ApiResult<Many> {
    let users = sqlx::query_as::<_, User>(
        "SELECT id, name, email, age FROM users ORDER BY id LIMIT $1 OFFSET $2",
    )
    .bind(page.limit.unwrap_or(100).clamp(0, 1000))
    .bind(page.offset.unwrap_or(0).max(0))
    .fetch_all(&db)
    .await
    .map_err(internal)?;
    Ok(Json(Many { users }))
}

async fn create_user(State(db): State<PgPool>, Json(new): Json<NewUser>) -> ApiResult<One> {
    let user = sqlx::query_as::<_, User>(
        "INSERT INTO users (name, email, age) VALUES ($1, $2, $3) RETURNING id, name, email, age",
    )
    .bind(new.name)
    .bind(new.email)
    .bind(new.age)
    .fetch_one(&db)
    .await
    .map_err(internal)?;
    Ok(Json(One { user }))
}

#[tokio::main]
async fn main() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let pool_size = std::env::var("DB_POOL").ok().and_then(|v| v.parse().ok()).unwrap_or(32);
    let port = std::env::var("APP_PORT").unwrap_or_else(|_| "8080".into());
    let db = PgPoolOptions::new().max_connections(pool_size).connect(&url).await.expect("db");

    let app = Router::new()
        .route("/ping", get(ping))
        .route("/users", get(list_users).post(create_user))
        .route("/users/{id}", get(get_user))
        .with_state(db);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
