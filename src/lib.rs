use actix_web::{Error, FromRequest, HttpRequest, error::ErrorUnauthorized};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use dotenvy::dotenv;
use jsonwebtoken::{decode, DecodingKey, Validation, Algorithm};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthUser {
    pub user_id: i32,
    pub auth0_id: String,
    pub email: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Auth0Claims {
    pub sub: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub iss: String,
    pub aud: Vec<String>,
    pub exp: usize,
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
            
            let jwks_uri = std::env::var("AUTH0_JWKS_URI")
                .unwrap_or_else(|_| "https://dev-example.auth0.com/.well-known/jwks.json".to_string());
            
            let jwks_response = reqwest::get(&jwks_uri)
                .await
                .map_err(|_| ErrorUnauthorized("Failed to fetch JWKS"))?
                .text()
                .await
                .map_err(|_| ErrorUnauthorized("Failed to read JWKS"))?;
            
            let jwks: serde_json::Value = serde_json::from_str(&jwks_response)
                .map_err(|_| ErrorUnauthorized("Invalid JWKS format"))?;
            
            let keys = jwks["keys"].as_array()
                .ok_or_else(|| ErrorUnauthorized("No keys in JWKS"))?;
            
            if keys.is_empty() {
                return Err(ErrorUnauthorized("Empty JWKS"));
            }
            
            let first_key = &keys[0];
            let n = first_key["n"].as_str()
                .ok_or_else(|| ErrorUnauthorized("Missing n in key"))?;
            let e = first_key["e"].as_str()
                .ok_or_else(|| ErrorUnauthorized("Missing e in key"))?;
            
            let decoding_key = DecodingKey::from_rsa_components(n, e)
                .map_err(|_| ErrorUnauthorized("Failed to create decoding key"))?;
            
            let mut validation = Validation::new(Algorithm::RS256);
            validation.validate_exp = true;
            
            let token_data = decode::<Auth0Claims>(token, &decoding_key, &validation)
                .map_err(|e| {
                    eprintln!("Token validation error: {:?}", e);
                    ErrorUnauthorized("Invalid token")
                })?;
            
            let pool = pool.ok_or_else(|| ErrorUnauthorized("Database not available"))?;
            
            let user_result = sqlx::query!(
                "SELECT user_id, auth0_id, email, name FROM users WHERE auth0_id = $1",
                token_data.claims.sub
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
                    let new_user = sqlx::query!(
                        "INSERT INTO users (auth0_id, email, name) VALUES ($1, $2, $3) RETURNING user_id, auth0_id, email, name",
                        token_data.claims.sub,
                        token_data.claims.email,
                        token_data.claims.name
                    )
                    .fetch_one(pool.get_ref())
                    .await
                    .map_err(|_| ErrorUnauthorized("Failed to create user"))?;
                    
                    Ok(AuthUser {
                        user_id: new_user.user_id,
                        auth0_id: new_user.auth0_id,
                        email: Some(new_user.email),
                        name: Some(new_user.name),
                    })
                }
            }
        })
    }
}

pub async fn db() -> PgPool {
    dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = sqlx::postgres::PgPool::connect(&database_url).await.unwrap();
    pool
}
