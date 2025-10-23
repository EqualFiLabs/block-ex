use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use mini_redis::server;
use redis::aio::ConnectionManager;
use serde_json::Value;
use tokio::{net::TcpListener, sync::oneshot};
use tower::ServiceExt;

fn normalize_block(block: &Value, base_height: i64) -> Value {
    let height = block
        .get("height")
        .and_then(Value::as_i64)
        .expect("block height");
    serde_json::json!({
        "height": height,
        "height_offset": base_height - height,
        "hash": block.get("hash").cloned().unwrap_or(Value::Null),
        "ts": block.get("ts").cloned().unwrap_or(Value::Null),
        "size_bytes": block.get("size_bytes").cloned().unwrap_or(Value::Null),
        "major_version": block
            .get("major_version")
            .cloned()
            .unwrap_or(Value::Null),
        "minor_version": block
            .get("minor_version")
            .cloned()
            .unwrap_or(Value::Null),
        "tx_count": block.get("tx_count").cloned().unwrap_or(Value::Null),
        "reward_nanos": block
            .get("reward_nanos")
            .cloned()
            .unwrap_or(Value::Null),
    })
}

#[tokio::test]
async fn health_and_blocks_routes_exist() {
    let db = match std::env::var("DATABASE_URL") {
        Ok(v) => v,
        Err(_) => return,
    };

    let pool = sqlx::PgPool::connect(&db).await.unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let server_task = tokio::spawn(async move {
        let shutdown = async {
            let _ = shutdown_rx.await;
        };
        let _ = server::run(listener, shutdown).await;
    });
    let redis_url = format!("redis://{}", addr);
    let client = redis::Client::open(redis_url).unwrap();
    let cache = ConnectionManager::new(client).await.unwrap();
    let state = api::state::AppState {
        db: pool.clone(),
        cache,
    };

    let stats = sqlx::query!(
        r#"
SELECT MIN(height) AS "min_height?", MAX(height) AS "max_height?"
FROM public.blocks
"#
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let (min_height, max_height) = match (stats.min_height, stats.max_height) {
        (Some(min_h), Some(max_h)) => (min_h, max_h),
        _ => return,
    };

    let app = api::routes::v1_router().with_state(state.clone());

    let limit = 5_i64;
    let first_page = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/blocks?limit={limit}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first_page.status(), StatusCode::OK);
    let body = to_bytes(first_page.into_body(), usize::MAX).await.unwrap();
    let blocks: Vec<Value> = serde_json::from_slice(&body).unwrap();
    assert!(!blocks.is_empty());
    let heights: Vec<i64> = blocks
        .iter()
        .map(|v| v.get("height").and_then(Value::as_i64).unwrap())
        .collect();
    assert!(heights.windows(2).all(|w| w[0] > w[1]));

    let next_start = heights.last().unwrap() - 1;
    let next_page = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/blocks?start={next_start}&limit={limit}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(next_page.status(), StatusCode::OK);
    let next_body = to_bytes(next_page.into_body(), usize::MAX).await.unwrap();
    let next_blocks: Vec<Value> = serde_json::from_slice(&next_body).unwrap();
    if !next_blocks.is_empty() {
        let next_heights: Vec<i64> = next_blocks
            .iter()
            .map(|v| v.get("height").and_then(Value::as_i64).unwrap())
            .collect();
        assert!(next_heights.iter().all(|h| *h <= next_start));
        assert!(next_heights.windows(2).all(|w| w[0] > w[1]));
    }

    let end_height = std::cmp::min(min_height + 2, max_height);
    let earliest_heights: Vec<i64> = sqlx::query_scalar!(
        r#"
SELECT height FROM public.blocks
WHERE height BETWEEN $1 AND $2
ORDER BY height ASC
"#,
        min_height,
        end_height
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    if earliest_heights.is_empty() {
        return;
    }
    let stable_window = earliest_heights.len() as i64;
    let stable_start = *earliest_heights.last().unwrap();

    let stable_page = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/blocks?start={stable_start}&limit={stable_window}"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stable_page.status(), StatusCode::OK);
    let stable_body = to_bytes(stable_page.into_body(), usize::MAX).await.unwrap();
    let stable_blocks: Vec<Value> = serde_json::from_slice(&stable_body).unwrap();
    assert_eq!(stable_blocks.len() as i64, stable_window);
    let stable_heights: Vec<i64> = stable_blocks
        .iter()
        .map(|v| v.get("height").and_then(Value::as_i64).unwrap())
        .collect();
    assert!(stable_heights.windows(2).all(|w| w[0] > w[1]));
    assert_eq!(stable_heights[0], stable_start);
    assert_eq!(
        *stable_heights.last().unwrap(),
        stable_start - (stable_window - 1)
    );

    let normalized: Vec<Value> = stable_blocks
        .iter()
        .map(|block| normalize_block(block, stable_start))
        .collect();
    insta::assert_json_snapshot!("blocks_pagination_window", normalized);

    let detail_res = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/block/{min_height}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(detail_res.status(), StatusCode::OK);
    let detail_body = to_bytes(detail_res.into_body(), usize::MAX).await.unwrap();
    let detail_block: Value = serde_json::from_slice(&detail_body).unwrap();
    let detail_normalized = normalize_block(&detail_block, min_height);
    insta::assert_json_snapshot!("block_detail_min_height", detail_normalized);

    let _ = shutdown_tx.send(());
    let _ = server_task.await;
}
