use sqlx::{PgPool, postgres::PgPoolOptions};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SQLError {
    #[error("connection failed: {0}")]
    ConnectionFailed(#[from] sqlx::Error),
}

/// A shared Postgres connection pool.
#[derive(Clone)]
pub struct SQLConnector {
    pool: PgPool,
}

impl SQLConnector {
    /// Connects with a pool of 5 connections.
    pub async fn connect(url: &str) -> Result<Self, SQLError> {
        Self::connect_with_pool_size(url, 5).await
    }

    pub async fn connect_with_pool_size(url: &str, max_connections: u32) -> Result<Self, SQLError> {
        let pool = PgPoolOptions::new().max_connections(max_connections).connect(url).await?;
        Ok(Self { pool })
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }
}
