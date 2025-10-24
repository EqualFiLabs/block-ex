use axum::{
    body::Body,
    http::{HeaderValue, StatusCode},
    response::Response,
};
use redis::aio::ConnectionManager;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tracing::debug;

pub fn json_ok<T: Serialize>(data: T) -> Response {
    let payload = serde_json::to_vec(&data).unwrap();
    make_json_response(payload, StatusCode::OK)
}

pub fn json_err(code: u16, msg: &str) -> Response {
    let payload = serde_json::to_vec(&serde_json::json!({"error": msg})).unwrap();
    make_json_response(payload, StatusCode::from_u16(code).unwrap())
}

pub async fn cached_json<T: Serialize>(
    cache: &ConnectionManager,
    key: &str,
    data: &T,
    ttl_secs: usize,
) -> Response {
    let payload = serde_json::to_vec(data).unwrap();
    let mut conn = cache.clone();
    let _: Result<(), _> = redis::cmd("SETEX")
        .arg(key)
        .arg(ttl_secs)
        .arg(&payload)
        .query_async::<_, ()>(&mut conn)
        .await;
    make_json_response(payload, StatusCode::OK)
}

pub async fn cached_response(cache: &ConnectionManager, key: &str) -> Option<Response> {
    let mut conn = cache.clone();
    match redis::cmd("GET")
        .arg(key)
        .query_async::<_, Option<Vec<u8>>>(&mut conn)
        .await
    {
        Ok(Some(bytes)) => {
            debug!(cache_key = key, "cache hit");
            Some(make_json_response(bytes, StatusCode::OK))
        }
        _ => None,
    }
}

fn make_json_response(payload: Vec<u8>, status: StatusCode) -> Response {
    let etag = hex::encode(Sha256::digest(&payload));
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .header(
            "ETag",
            HeaderValue::from_str(&format!("W/\"{etag}\"")).unwrap(),
        )
        .body(Body::from(payload))
        .unwrap()
}

pub fn is_hex_64(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit())
}
