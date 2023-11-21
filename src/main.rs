use axum::{
    extract,
    response::IntoResponse,
    routing::{delete, get},
    Json, Router,
};
use redis::{AsyncCommands, JsonAsyncCommands};
use redis_macros::{FromRedisValue, ToRedisArgs};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::HashMap, net::SocketAddr};
use ulid::Ulid;

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt::init();

    let redis: redis::Client;
    {
        use std::env;
        redis = redis::Client::open(env::var("REDIS_URL").expect("missing REDIS_URL")).unwrap();
    }

    let redis_conn = redis
        .get_multiplexed_tokio_connection()
        .await
        .expect("failed to establish connection");

    let app = Router::new()
        .route("/todos", get(list_todos).post(create_todo))
        .route("/todos/:id", delete(delete_todo).patch(update_todo))
        .with_state(redis_conn);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

#[derive(Deserialize, Serialize, FromRedisValue, ToRedisArgs)]
struct Todo {
    id: String,
    task: String,
    created_at: chrono::DateTime<chrono::Utc>,
    completed: bool,
}

async fn list_todos(
    extract::State(mut redis): extract::State<redis::aio::MultiplexedConnection>,
) -> impl IntoResponse {
    let redis_macros::Json(todos): redis_macros::Json<HashMap<String, Todo>> =
        redis.json_get("todos", "$").await.unwrap();

    Json(todos)
}

#[derive(Serialize, Deserialize)]
struct CreateTodo {
    task: String,
}

async fn create_todo(
    extract::State(mut redis): extract::State<redis::aio::MultiplexedConnection>,
    extract::Json(payload): extract::Json<CreateTodo>,
) -> impl IntoResponse {
    if !redis.exists::<&str, bool>("todos").await.unwrap() {
        redis
            .json_set::<_, _, _, ()>("todos", "$", &json!({}))
            .await
            .unwrap();
    }

    let id = Ulid::new().to_string();

    let todo = Todo {
        id,
        task: payload.task,
        created_at: chrono::Utc::now(),
        completed: false,
    };

    let result = redis
        .json_set::<_, _, _, ()>("todos", format!("$.{}", todo.id), &todo)
        .await;

    match result {
        Ok(_) => (
            axum::http::StatusCode::CREATED,
            Json(serde_json::to_value(todo).unwrap()),
        ),
        Err(err) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to create todo: {err}") })),
        ),
    }
}

async fn delete_todo(
    extract::Path(id): extract::Path<String>,
    extract::State(mut redis): extract::State<redis::aio::MultiplexedConnection>,
) -> impl IntoResponse {
    if !redis.exists::<&str, bool>("todos").await.unwrap() {
        redis
            .json_set::<_, _, _, ()>("todos", "$", &json!({}))
            .await
            .unwrap();
    }

    redis
        .json_del::<_, _, ()>("todos", format!("$.{}", id))
        .await
        .unwrap();

    (axum::http::StatusCode::OK, Json(json!({ "status": "ok" })))
}

async fn update_todo(
    extract::Path(id): extract::Path<String>,
    extract::State(mut redis): extract::State<redis::aio::MultiplexedConnection>,
) -> impl IntoResponse {
    if !redis.exists::<&str, bool>("todos").await.unwrap() {
        redis
            .json_set::<_, _, _, ()>("todos", "$", &json!({}))
            .await
            .unwrap();
    }

    let result = redis.json_get("todos", format!("$.{id}")).await;

    match result {
        Ok(redis_macros::Json(todo)) => {
            let todo: Todo = todo;
            let todo = Todo {
                completed: !todo.completed,
                ..todo
            };

            redis
                .json_set::<_, _, _, ()>("todos", format!("$.{id}"), &todo)
                .await
                .unwrap();

            (axum::http::StatusCode::OK, Json(json!({ "status": "ok" })))
        }
        Err(err) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to update todo: {err}") })),
        ),
    }
}
