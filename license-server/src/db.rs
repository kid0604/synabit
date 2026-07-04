use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::env;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct License {
    pub id: i64,
    pub license_key: Option<String>,
    pub r#type: String, // 'trial', 'pro_monthly', 'pro_annual'
    pub status: String,
    pub max_devices: i64,
    pub expires_at: chrono::NaiveDateTime,
    pub customer_email: Option<String>,
    pub payment_id: Option<String>,
    pub created_at: chrono::NaiveDateTime,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Device {
    pub id: i64,
    pub license_id: i64,
    pub hwid: String,
    pub device_name: Option<String>,
    pub activated_at: chrono::NaiveDateTime,
    pub last_heartbeat: Option<chrono::NaiveDateTime>,
}

#[derive(Clone)]
pub struct Db {
    pub pool: SqlitePool,
}

impl Db {
    pub async fn new() -> Result<Self, sqlx::Error> {
        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:license.db".to_string());
        
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Db { pool })
    }
}
