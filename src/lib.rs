use actix_web::{Error, FromRequest, HttpRequest, error::ErrorUnauthorized};
use dotenvy::dotenv;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::LazyLock;
use std::time::Duration;

// Cache for validated tokens (token -> claims) - 5 minute TTL
static TOKEN_CACHE: LazyLock<Cache<String, Auth0Claims>> = LazyLock::new(|| {
    Cache::builder()
        .time_to_live(Duration::from_secs(300))
        .max_capacity(1000)
        .build()
});

// Cache for JWKS - 1 hour TTL
static JWKS_CACHE: LazyLock<Cache<String, String>> = LazyLock::new(|| {
    Cache::builder()
        .time_to_live(Duration::from_secs(3600))
        .max_capacity(10)
        .build()
});

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthUser {
    pub user_id: i32,
    pub auth0_id: String,
    pub email: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Auth0Claims {
    pub sub: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub iss: Option<String>,
    pub aud: Option<serde_json::Value>,
    pub exp: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UserInfoResponse {
    sub: String,
    email: Option<String>,
    name: Option<String>,
}

impl FromRequest for AuthUser {
    type Error = Error;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, _payload: &mut actix_web::dev::Payload) -> Self::Future {
        let auth_header = req.headers().get("Authorization").cloned();
        let pool = req.app_data::<actix_web::web::Data<PgPool>>().cloned();

        Box::pin(async move {
            let auth_header = match auth_header {
                Some(h) => h,
                None => return Err(ErrorUnauthorized("No Authorization header")),
            };

            let auth_str = match auth_header.to_str() {
                Ok(s) => s,
                Err(_) => return Err(ErrorUnauthorized("Invalid Authorization header")),
            };

            if !auth_str.starts_with("Bearer ") {
                return Err(ErrorUnauthorized("Invalid Authorization format"));
            }

            let token = &auth_str[7..];
            let pool = pool.ok_or_else(|| ErrorUnauthorized("Database not available"))?;

            // Check token cache first
            if let Some(cached_claims) = TOKEN_CACHE.get(token).await {
                return get_or_create_user(&pool, cached_claims).await;
            }

            let auth0_domain = std::env::var("AUTH0_DOMAIN")
                .unwrap_or_else(|_| "dev-example.auth0.com".to_string());

            // Try to validate as JWT first, fall back to userinfo endpoint for opaque tokens
            let claims = match validate_jwt(token, &auth0_domain).await {
                Ok(claims) => claims,
                Err(_) => {
                    // Token might be opaque, try userinfo endpoint
                    validate_via_userinfo(token, &auth0_domain).await?
                }
            };

            // Cache the validated token
            TOKEN_CACHE.insert(token.to_string(), claims.clone()).await;

            get_or_create_user(&pool, claims).await
        })
    }
}

async fn get_or_create_user(
    pool: &actix_web::web::Data<PgPool>,
    claims: Auth0Claims,
) -> Result<AuthUser, Error> {
    let user_result = sqlx::query!(
        "SELECT user_id, auth0_id, email, name FROM users WHERE auth0_id = $1",
        claims.sub
    )
    .fetch_optional(pool.get_ref())
    .await
    .map_err(|_| ErrorUnauthorized("Database error"))?;

    match user_result {
        Some(user) => Ok(AuthUser {
            user_id: user.user_id,
            auth0_id: user.auth0_id,
            email: Some(user.email),
            name: Some(user.name),
        }),
        None => {
            // Provide defaults for required fields if not present in claims
            let email = claims
                .email
                .unwrap_or_else(|| format!("{}@unknown.local", claims.sub));
            let name = claims.name.unwrap_or_else(|| "Unknown User".to_string());

            let new_user = sqlx::query!(
                "INSERT INTO users (auth0_id, email, name) VALUES ($1, $2, $3) RETURNING user_id, auth0_id, email, name",
                claims.sub,
                email,
                name
            )
            .fetch_one(pool.get_ref())
            .await
            .map_err(|e| {
                eprintln!("Failed to create user: {:?}", e);
                ErrorUnauthorized("Failed to create user")
            })?;

            Ok(AuthUser {
                user_id: new_user.user_id,
                auth0_id: new_user.auth0_id,
                email: Some(new_user.email),
                name: Some(new_user.name),
            })
        }
    }
}

async fn validate_jwt(token: &str, auth0_domain: &str) -> Result<Auth0Claims, Error> {
    let jwks_uri = format!("https://{}/.well-known/jwks.json", auth0_domain);

    // Try to get JWKS from cache first
    let jwks_response = match JWKS_CACHE.get(&jwks_uri).await {
        Some(cached) => cached,
        None => {
            let response = reqwest::get(&jwks_uri)
                .await
                .map_err(|_| ErrorUnauthorized("Failed to fetch JWKS"))?
                .text()
                .await
                .map_err(|_| ErrorUnauthorized("Failed to read JWKS"))?;

            JWKS_CACHE.insert(jwks_uri.clone(), response.clone()).await;
            response
        }
    };

    let jwks: serde_json::Value = serde_json::from_str(&jwks_response)
        .map_err(|_| ErrorUnauthorized("Invalid JWKS format"))?;

    let keys = jwks["keys"]
        .as_array()
        .ok_or_else(|| ErrorUnauthorized("No keys in JWKS"))?;

    if keys.is_empty() {
        return Err(ErrorUnauthorized("Empty JWKS"));
    }

    let first_key = &keys[0];
    let n = first_key["n"]
        .as_str()
        .ok_or_else(|| ErrorUnauthorized("Missing n in key"))?;
    let e = first_key["e"]
        .as_str()
        .ok_or_else(|| ErrorUnauthorized("Missing e in key"))?;

    let decoding_key = DecodingKey::from_rsa_components(n, e)
        .map_err(|_| ErrorUnauthorized("Failed to create decoding key"))?;

    let mut validation = Validation::new(Algorithm::RS256);
    validation.validate_exp = true;
    validation.set_issuer(&[format!("https://{}/", auth0_domain)]);
    validation.validate_aud = false;

    let token_data = decode::<Auth0Claims>(token, &decoding_key, &validation).map_err(|e| {
        eprintln!("JWT validation error: {:?}", e);
        ErrorUnauthorized("Invalid JWT token")
    })?;

    Ok(token_data.claims)
}

async fn validate_via_userinfo(token: &str, auth0_domain: &str) -> Result<Auth0Claims, Error> {
    let userinfo_url = format!("https://{}/userinfo", auth0_domain);

    let client = reqwest::Client::new();
    let response = client
        .get(&userinfo_url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| {
            eprintln!("Userinfo request error: {:?}", e);
            ErrorUnauthorized("Failed to validate token")
        })?;

    if !response.status().is_success() {
        eprintln!("Userinfo returned status: {}", response.status());
        return Err(ErrorUnauthorized("Invalid token"));
    }

    let user_info: UserInfoResponse = response.json().await.map_err(|e| {
        eprintln!("Userinfo parse error: {:?}", e);
        ErrorUnauthorized("Failed to parse userinfo")
    })?;

    Ok(Auth0Claims {
        sub: user_info.sub,
        email: user_info.email,
        name: user_info.name,
        iss: None,
        aud: None,
        exp: None,
    })
}

pub async fn db() -> PgPool {
    dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    // Validate URL format before attempting connection
    if !database_url.starts_with("postgres://") && !database_url.starts_with("postgresql://") {
        panic!(
            "DATABASE_URL must be a valid PostgreSQL URL starting with postgres:// or postgresql://. Got: {}",
            if database_url.len() > 20 {
                &database_url[..20]
            } else {
                &database_url
            }
        );
    }

    sqlx::postgres::PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database")
}
