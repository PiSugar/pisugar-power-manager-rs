use anyhow::Result;
use base64::Engine;
use jsonwebtoken::{decode, DecodingKey, Validation};
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::SystemTime;

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
}

pub fn read_or_create_jwt_secret(path: &str) -> Result<String> {
    let p: &Path = Path::new(path);
    if p.exists() {
        let secret = fs::read_to_string(p)?;
        Ok(secret)
    } else {
        let mut secret_bytes = [0; 32];
        let base64_engine = base64::engine::general_purpose::STANDARD;
        rand::fill(&mut secret_bytes);
        let secret = base64_engine.encode(secret_bytes);
        fs::write(p, &secret)?;
        Ok(secret)
    }
}

pub fn generate_jwt(username: &str, secret: &str, expire_seconds: u64) -> Result<String> {
    let expiration = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_secs() + expire_seconds;

    let claims = Claims {
        sub: username.to_owned(),
        exp: expiration as usize,
    };

    let token = encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_ref()))?;

    Ok(token)
}

pub fn verify_jwt(token: &str, secret: &str) -> Result<bool> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_ref()),
        &Validation::default(),
    )?;

    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_secs() as usize;
    if token_data.claims.exp < now {
        return Ok(false);
    }

    Ok(true)
}
