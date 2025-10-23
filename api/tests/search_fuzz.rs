use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use mini_redis::server;
use rand::{distributions::Alphanumeric, Rng};
use redis::aio::ConnectionManager;
use tokio::{net::TcpListener, sync::oneshot};
use tower::ServiceExt;

#[tokio::test]
async fn fuzz_search_inputs() {
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

    let client = redis::Client::open(format!("redis://{}", addr)).unwrap();
    let cache = ConnectionManager::new(client).await.unwrap();
    let state = api::state::AppState { db: pool, cache };
    let app = api::routes::v1_router().with_state(state);

    for _ in 0..50 {
        let random: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();
        let req = Request::builder()
            .uri(format!("/api/v1/search?q={random}"))
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_ne!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    let _ = shutdown_tx.send(());
    let _ = server_task.await;
}
