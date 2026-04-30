use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::mysql::MySqlPoolOptions;
use sqlx::MySqlPool;
use std::env;

#[derive(Clone)]
struct AppState {
    db: MySqlPool,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
struct Todo {
    id: Option<i32>,
    name: String,
    description: String,
    finished: bool,
}

#[derive(Debug, Deserialize)]
struct CreateTodo {
    name: String,
    description: String,
    finished: bool,
}

#[derive(Debug, Deserialize)]
struct UpdateTodo {
    name: String,
    description: String,
    finished: bool,
}

async fn health_check() -> &'static str {
    "API rodando"
}

async fn create_todo(
    State(state): State<AppState>,
    Json(payload): Json<CreateTodo>,
) -> Result<Json<Todo>, String> {
    let result = sqlx::query(
        r#"
        INSERT INTO todos (name, description, finished)
        VALUES (?, ?, ?)
        "#,
    )
    .bind(&payload.name)
    .bind(&payload.description)
    .bind(payload.finished)
    .execute(&state.db)
    .await
    .map_err(|err| err.to_string())?;

    let todo = Todo {
        id: Some(result.last_insert_id() as i32),
        name: payload.name,
        description: payload.description,
        finished: payload.finished,
    };

    Ok(Json(todo))
}

async fn get_todos(
    State(state): State<AppState>,
) -> Result<Json<Vec<Todo>>, String> {
    let todos = sqlx::query_as::<_, Todo>(
        r#"
        SELECT id, name, description, finished
        FROM todos
        ORDER BY id
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|err| err.to_string())?;

    Ok(Json(todos))
}

async fn get_todo_by_id(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i32>,
) -> Result<Json<Todo>, String> {
    let todo = sqlx::query_as::<_, Todo>(
        r#"
        SELECT id, name, description, finished
        FROM todos
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(|err| err.to_string())?;

    Ok(Json(todo))
}

async fn update_todo(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i32>,
    Json(payload): Json<UpdateTodo>,
) -> Result<Json<Todo>, String> {
    sqlx::query(
        r#"
        UPDATE todos
        SET name = ?, description = ?, finished = ?
        WHERE id = ?
        "#,
    )
    .bind(&payload.name)
    .bind(&payload.description)
    .bind(payload.finished)
    .bind(id)
    .execute(&state.db)
    .await
    .map_err(|err| err.to_string())?;

    let todo = sqlx::query_as::<_, Todo>(
        r#"
        SELECT id, name, description, finished
        FROM todos
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(|err| err.to_string())?;

    Ok(Json(todo))
}

async fn delete_todo(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i32>,
) -> Result<StatusCode, String> {
    sqlx::query(
        r#"
        DELETE FROM todos
        WHERE id = ?
        "#,
    )
    .bind(id)
    .execute(&state.db)
    .await
    .map_err(|err| err.to_string())?;

    Ok(StatusCode::NO_CONTENT)
}

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    dotenvy::dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL nao foi definida");

    let pool = MySqlPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    println!("Banco conectado e migrations aplicadas");

    let state = AppState { db: pool };

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/todos", post(create_todo).get(get_todos))
        .route("/todos/:id", get(get_todo_by_id).put(update_todo).delete(delete_todo))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

    println!("Servidor rodando em http://127.0.0.1:3000");

    axum::serve(listener, app).await.unwrap();

    Ok(())
}
