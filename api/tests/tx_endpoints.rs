use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use mini_redis::server;
use redis::aio::ConnectionManager;
use serde_json::Value;
use tokio::{net::TcpListener, sync::oneshot};
use tower::ServiceExt;

#[tokio::test]
async fn tx_endpoint_includes_inputs_outputs() {
    let db = match std::env::var("DATABASE_URL") {
        Ok(v) => v,
        Err(_) => return,
    };

    let pool = match sqlx::PgPool::connect(&db).await {
        Ok(p) => p,
        Err(_) => return,
    };

    let tx_row = match sqlx::query!(
        r#"
SELECT encode(t.tx_hash,'hex') AS hash,
       t.num_inputs,
       t.num_outputs
FROM public.txs t
WHERE t.num_inputs > 0
  AND t.num_outputs > 0
  AND EXISTS (
    SELECT 1 FROM public.outputs o WHERE o.tx_hash = t.tx_hash LIMIT 1
  )
  AND EXISTS (
    SELECT 1 FROM public.tx_inputs ti WHERE ti.tx_hash = t.tx_hash LIMIT 1
  )
ORDER BY t.block_timestamp ASC
LIMIT 1
"#
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(row)) => row,
        _ => return,
    };

    let hash = match tx_row.hash {
        Some(h) => h,
        None => return,
    };
    let expected_inputs = tx_row.num_inputs;
    let expected_outputs = tx_row.num_outputs;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let server_task = tokio::spawn(async move {
        let shutdown = async {
            let _ = shutdown_rx.await;
        };
        let _ = server::run(listener, shutdown).await;
    });

    let client = redis::Client::open(format!("redis://{}", addr)).unwrap();
    let cache = ConnectionManager::new(client).await.unwrap();
    let state = api::state::AppState {
        db: pool.clone(),
        cache,
    };
    let app = api::routes::v1_router().with_state(state);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/tx/{hash}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(
        json.get("hash").and_then(Value::as_str),
        Some(hash.as_str())
    );
    assert_eq!(
        json.get("num_inputs").and_then(Value::as_i64),
        Some(expected_inputs as i64)
    );
    assert_eq!(
        json.get("num_outputs").and_then(Value::as_i64),
        Some(expected_outputs as i64)
    );

    let inputs = json
        .get("inputs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert_eq!(inputs.len() as i32, expected_inputs);
    if let Some(first) = inputs.first() {
        let key_image = first.get("key_image").and_then(Value::as_str).unwrap();
        assert_eq!(key_image.len(), 64);
    }

    let outputs = json
        .get("outputs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert_eq!(outputs.len() as i32, expected_outputs);
    if let Some(first) = outputs.first() {
        let commitment = first.get("commitment").and_then(Value::as_str).unwrap();
        assert!(!commitment.is_empty());
    }

    let invalid = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/tx/xyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);

    let _ = shutdown_tx.send(());
    let _ = server_task.await;
}

#[tokio::test]
async fn rings_endpoint_groups_members() {
    let db = match std::env::var("DATABASE_URL") {
        Ok(v) => v,
        Err(_) => return,
    };

    let pool = match sqlx::PgPool::connect(&db).await {
        Ok(p) => p,
        Err(_) => return,
    };

    let tx_hash_row = match sqlx::query!(
        r#"
SELECT encode(tx_hash,'hex') AS hash
FROM public.rings
GROUP BY tx_hash
ORDER BY COUNT(*) DESC
LIMIT 1
"#
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(row)) => row,
        _ => return,
    };

    let hash = match tx_hash_row.hash {
        Some(h) => h,
        None => return,
    };

    let inputs = match sqlx::query!(
        r#"
SELECT idx,
       ring_size
FROM public.tx_inputs
WHERE tx_hash = decode($1,'hex')
ORDER BY idx ASC
"#,
        hash.as_str()
    )
    .fetch_all(&pool)
    .await
    {
        Ok(rows) if !rows.is_empty() => rows,
        _ => return,
    };

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let server_task = tokio::spawn(async move {
        let shutdown = async {
            let _ = shutdown_rx.await;
        };
        let _ = server::run(listener, shutdown).await;
    });

    let client = redis::Client::open(format!("redis://{}", addr)).unwrap();
    let cache = ConnectionManager::new(client).await.unwrap();
    let state = api::state::AppState { db: pool, cache };
    let app = api::routes::v1_router().with_state(state);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/tx/{hash}/rings"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let rings: Value = serde_json::from_slice(&body).unwrap();
    let ring_sets = rings.as_array().cloned().unwrap_or_default();
    assert_eq!(ring_sets.len(), inputs.len());

    for (set, expected) in ring_sets.iter().zip(inputs.iter()) {
        let input_idx = set.get("input_idx").and_then(Value::as_i64).unwrap();
        assert_eq!(input_idx as i32, expected.idx);
        let members = set.get("members").and_then(Value::as_array).unwrap();
        assert_eq!(members.len(), expected.ring_size as usize);
        let indices: Vec<i64> = members
            .iter()
            .filter_map(|m| m.get("ring_index").and_then(Value::as_i64))
            .collect();
        let mut sorted = indices.clone();
        sorted.sort_unstable();
        assert_eq!(indices, sorted);
    }

    let invalid = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/tx/xyz/rings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);

    let _ = shutdown_tx.send(());
    let _ = server_task.await;
}
