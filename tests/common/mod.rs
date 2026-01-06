use sqlx::PgPool;
use testcontainers::runners::AsyncRunner;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres;

pub struct TestContext {
    pub pool: PgPool,
    pub _container: Option<ContainerAsync<Postgres>>,
}

pub async fn setup_test_db() -> TestContext {
    // Check if TEST_DATABASE_URL is set - if so, use existing database
    if let Ok(database_url) = std::env::var("TEST_DATABASE_URL") {
        let pool = PgPool::connect(&database_url)
            .await
            .expect("Failed to connect to test database");

        // Clean up any existing test data
        cleanup_test_data(&pool).await;

        // Skip schema creation when using existing database - assume it already exists
        return TestContext {
            pool,
            _container: None,
        };
    }

    // Otherwise, start PostgreSQL container
    let container = Postgres::default()
        .start()
        .await
        .expect("Failed to start postgres container. Either install Docker or set TEST_DATABASE_URL");
    
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("Failed to get container port");
    
    let database_url = format!(
        "postgres://postgres:postgres@127.0.0.1:{}/postgres",
        port
    );

    // Connect to database
    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database");

    // Run schema
    let schema = include_str!("../../schema.sql");
    sqlx::raw_sql(schema)
        .execute(&pool)
        .await
        .expect("Failed to run schema");

    TestContext {
        pool,
        _container: Some(container),
    }
}

async fn cleanup_test_data(pool: &PgPool) {
    // Clean up in reverse order of foreign key dependencies
    let _ = sqlx::raw_sql("DELETE FROM contact_tags WHERE contact_id IN (SELECT contact_id FROM contacts WHERE user_id IN (SELECT user_id FROM users WHERE auth0_id LIKE 'test|%'))")
        .execute(pool)
        .await;
    let _ = sqlx::raw_sql("DELETE FROM interactions WHERE contact_id IN (SELECT contact_id FROM contacts WHERE user_id IN (SELECT user_id FROM users WHERE auth0_id LIKE 'test|%'))")
        .execute(pool)
        .await;
    let _ = sqlx::raw_sql("DELETE FROM occasions WHERE contact_id IN (SELECT contact_id FROM contacts WHERE user_id IN (SELECT user_id FROM users WHERE auth0_id LIKE 'test|%'))")
        .execute(pool)
        .await;
    let _ = sqlx::raw_sql("DELETE FROM contacts WHERE user_id IN (SELECT user_id FROM users WHERE auth0_id LIKE 'test|%')")
        .execute(pool)
        .await;
    let _ = sqlx::raw_sql("DELETE FROM tags WHERE user_id IN (SELECT user_id FROM users WHERE auth0_id LIKE 'test|%')")
        .execute(pool)
        .await;
    let _ = sqlx::raw_sql("DELETE FROM users WHERE auth0_id LIKE 'test|%'")
        .execute(pool)
        .await;
}

fn generate_unique_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("test|{}", nanos)
}

pub async fn setup_test_user(pool: &PgPool) -> i32 {
    let unique_id = generate_unique_id();
    
    let result = sqlx::query!(
        "INSERT INTO users (auth0_id, name, email) VALUES ($1, $2, $3) RETURNING user_id",
        unique_id,
        "Test User",
        format!("{}@example.com", unique_id)
    )
    .fetch_one(pool)
    .await
    .expect("Failed to create test user");

    result.user_id
}