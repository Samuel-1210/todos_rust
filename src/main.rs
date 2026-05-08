use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;
use sqlx::mysql::MySqlPoolOptions;
use std::env;

#[derive(Clone)]
struct AppState {
    db: MySqlPool,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}
#[derive(Debug)]
enum ApiError {
    NotFound(String),
    BadRequest(String),
    Database(sqlx::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            Self::NotFound(message) => (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse { error: message }),
            )
                .into_response(),

            Self::BadRequest(message) => (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse { error: message }),
            )
                .into_response(),

            Self::Database(error) => {
                eprintln!("Erro banco de dados: {error}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "Erro interno do servidor".to_string(),
                    }),
                )
                    .into_response()
            }
        }
    }
}

fn validate_todo_fields(name: String, description: String) -> ApiResult<(String, String)> {
    let name = name.trim().to_owned();
    let description = description.trim().to_owned();

    if name.is_empty() {
        return Err(ApiError::BadRequest("Nome não pode ser vazio".to_string()));
    }

    if description.is_empty() {
        return Err(ApiError::BadRequest(
            "Descrição não pode ser vazia".to_string(),
        ));
    }

    if name.len() < 3 {
        return Err(ApiError::BadRequest(
            "O nome deve ter no mínimo 3 caracteres".to_string(),
        ));
    }

    Ok((name, description))
}

type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug, Serialize, Deserialize)]
struct Todo {
    id: i32,
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
) -> ApiResult<(StatusCode, Json<Todo>)> {
    let CreateTodo {
        name,
        description,
        finished,
    } = payload;

    let (name, description) = validate_todo_fields(name, description)?;

    let result = sqlx::query!(
        r#"
        INSERT INTO todos (name, description, finished)
        VALUES (?, ?, ?)
        "#,
        name,
        description,
        finished
    )
    .execute(&state.db)
    .await
    .map_err(ApiError::Database)?;

    let todo = Todo {
        id: result.last_insert_id() as i32,
        name,
        description,
        finished,
    };

    Ok((StatusCode::CREATED, Json(todo)))
}

async fn get_todos(State(state): State<AppState>) -> ApiResult<Json<Vec<Todo>>> {
    let todos = sqlx::query_as!(
        Todo,
        r#"
        SELECT id, name, description, finished as `finished: _`
        FROM todos
        ORDER BY id
        "#
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::Database)?;

    Ok(Json(todos))
}

async fn get_todo_by_id(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i32>,
) -> ApiResult<Json<Todo>> {
    let todo = sqlx::query_as!(
        Todo,
        r#"
        SELECT id, name, description, finished as `finished: _`
        FROM todos
        WHERE id = ?
        "#,
        id
    )
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::Database)?
    .ok_or_else(|| ApiError::NotFound(format!("ToDo {id} não encontrado")))?;

    Ok(Json(todo))
}

async fn update_todo(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i32>,
    Json(payload): Json<UpdateTodo>,
) -> ApiResult<Json<Todo>> {
    let UpdateTodo {
        name,
        description,
        finished,
    } = payload;

    let (name, description) = validate_todo_fields(name, description)?;

    let result = sqlx::query!(
        r#"
        UPDATE todos
        SET name = ?, description = ?, finished = ?
        WHERE id = ?
        "#,
        name,
        description,
        finished,
        id
    )
    .execute(&state.db)
    .await
    .map_err(ApiError::Database)?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound(format!("ToDo {id} não encontrado")));
    }

    let todo = sqlx::query_as!(
        Todo,
        r#"
        SELECT id, name, description, finished as `finished: _`
        FROM todos
        WHERE id = ?
        "#,
        id
    )
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::Database)?
    .ok_or_else(|| ApiError::NotFound(format!("ToDo {id} não encontrado")))?;

    Ok(Json(todo))
}

async fn delete_todo(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i32>,
) -> ApiResult<StatusCode> {
    let result = sqlx::query!(
        r#"
        DELETE FROM todos
        WHERE id = ?
        "#,
        id
    )
    .execute(&state.db)
    .await
    .map_err(ApiError::Database)?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound(format!("ToDo {id} não encontrado")));
    }

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
        .route(
            "/todos/:id",
            get(get_todo_by_id).put(update_todo).delete(delete_todo),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

    println!("Servidor rodando em http://127.0.0.1:3000");

    axum::serve(listener, app).await.unwrap();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_todo_fields_rejects_empty_name() {
        let result = validate_todo_fields("   ".to_string(), "Descrição válida".to_string());

        assert!(
            matches!(result, Err(ApiError::BadRequest(message)) if message == "Nome não pode ser vazio")
        );
    }

    #[test]
    fn validate_todo_fields_rejects_empty_description() {
        let result = validate_todo_fields("Tarefa válida".to_string(), "   ".to_string());

        assert!(
            matches!(result, Err(ApiError::BadRequest(message)) if message == "Descrição não pode ser vazia")
        );
    }

    #[test]
    fn validate_todo_fields_rejects_short_name() {
        let result = validate_todo_fields("Ab".to_string(), "Descrição válida".to_string());

        assert!(
            matches!(result, Err(ApiError::BadRequest(message)) if message == "O nome deve ter no mínimo 3 caracteres")
        );
    }

    #[test]
    fn validate_todo_fields_trims_name_and_description() {
        let result = validate_todo_fields(
            "  Tarefa válida  ".to_string(),
            "  Descrição válida  ".to_string(),
        );

        let (name, description) = result.expect("validation should succeed");

        assert_eq!(name, "Tarefa válida");
        assert_eq!(description, "Descrição válida");
    }
}
