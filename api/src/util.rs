use axum::{
    body::Body,
    http::{HeaderValue, StatusCode},
    response::Response,
};
use redis::aio::ConnectionManager;
use serde::Serialize;
use sha2::{Digest, Sha256};

pub fn json_ok<T: Serialize>(data: T) -> Response {
    let payload = serde_json::to_vec(&data).unwrap();
    let etag = hex::encode(Sha256::digest(&payload));
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .header(
            "ETag",
            HeaderValue::from_str(&format!("W/\"{etag}\"")).unwrap(),
        )
        .body(Body::from(payload))
        .unwrap()
}

pub fn json_err(code: u16, msg: &str) -> Response {
    let payload = serde_json::to_vec(&serde_json::json!({"error": msg})).unwrap();
    Response::builder()
        .status(StatusCode::from_u16(code).unwrap())
        .header("Content-Type", "application/json")
        .body(Body::from(payload))
        .unwrap()
}

pub async fn cached_json<T: Serialize>(
    cache: &ConnectionManager,
    key: &str,
    data: &T,
    ttl_secs: usize,
) -> Response {
    let payload = serde_json::to_vec(data).unwrap();
    let _: Result<(), _> = redis::cmd("SETEX")
        .arg(key)
        .arg(ttl_secs)
        .arg(&payload)
        .query_async::<_, ()>(cache.clone())
        .await;
    json_ok(serde_json::from_slice::<serde_json::Value>(&payload).unwrap())
}
