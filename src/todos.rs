use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, MySql, QueryBuilder};

use crate::{
    error::{ApiError, ApiResult},
    state::AppState,
};

fn validate_todo_fields(name: String, description: String) -> ApiResult<(String, String)> {
    let name = name.trim().to_owned();
    let description = description.trim().to_owned();

    if name.is_empty() {
        return Err(ApiError::BadRequest("Nome nao pode ser vazio".to_string()));
    }

    if description.is_empty() {
        return Err(ApiError::BadRequest(
            "Descricao nao pode ser vazia".to_string(),
        ));
    }

    if name.len() < 3 {
        return Err(ApiError::BadRequest(
            "O nome deve ter no minimo 3 caracteres".to_string(),
        ));
    }

    Ok((name, description))
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub(crate) struct Todo {
    id: i32,
    name: String,
    description: String,
    finished: bool,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct TodoQueryParams {
    search: Option<String>,
    finished: Option<bool>,
    limit: Option<i64>,
    page: Option<i64>,
    sort_by: Option<String>,
    order_by: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateTodo {
    name: String,
    description: String,
    finished: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UpdateTodo {
    name: String,
    description: String,
    finished: bool,
}

pub(crate) async fn create_todo(
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

pub(crate) async fn get_todos(
    State(state): State<AppState>,
    Query(params): Query<TodoQueryParams>,
) -> ApiResult<Json<Vec<Todo>>> {
    let mut query_builder =
        QueryBuilder::<MySql>::new("SELECT id, name, description, finished FROM todos");
    let mut has_where = false;

    if let Some(finished) = params.finished {
        query_builder.push(" WHERE finished = ").push_bind(finished);
        has_where = true;
    }

    if let Some(search) = params.search {
        let search = search.trim();

        if !search.is_empty() {
            let pattern = format!("%{search}%");

            if has_where {
                query_builder.push(" AND ");
            } else {
                query_builder.push(" WHERE ");
            }

            query_builder
                .push("(name LIKE ")
                .push_bind(pattern.clone())
                .push(" OR description LIKE ")
                .push_bind(pattern)
                .push(")");
        }
    }

    let sort_column = match params.sort_by.as_deref() {
        Some("name") => "name",
        Some("description") => "description",
        Some("finished") => "finished",
        _ => "id",
    };

    let sort_order = match params.order_by.as_deref() {
        Some(o) if o.eq_ignore_ascii_case("desc") => "DESC",
        _ => "ASC",
    };

    query_builder.push(" ORDER BY ");
    query_builder.push(sort_column);
    query_builder.push(" ");
    query_builder.push(sort_order);

    let limit = params.limit.unwrap_or(20);
    let page = params.page.unwrap_or(1);
    if page < 1 {
        return Err(ApiError::BadRequest(
            "page deve ser maior ou igual a 1".to_string(),
        ));
    }
    let offset = (page - 1) * limit;

    query_builder.push(" LIMIT ").push_bind(limit);
    query_builder.push(" OFFSET ").push_bind(offset);

    let todos = query_builder
        .build_query_as::<Todo>()
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::Database)?;

    Ok(Json(todos))
}

pub(crate) async fn get_todo_by_id(
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
    .ok_or_else(|| ApiError::NotFound(format!("ToDo {id} nao encontrado")))?;

    Ok(Json(todo))
}

pub(crate) async fn update_todo(
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
        return Err(ApiError::NotFound(format!("ToDo {id} nao encontrado")));
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
    .ok_or_else(|| ApiError::NotFound(format!("ToDo {id} nao encontrado")))?;

    Ok(Json(todo))
}

pub(crate) async fn delete_todo(
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
        return Err(ApiError::NotFound(format!("ToDo {id} nao encontrado")));
    }

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_todo_fields_rejects_empty_name() {
        let result = validate_todo_fields("   ".to_string(), "Descricao valida".to_string());

        assert!(
            matches!(result, Err(ApiError::BadRequest(message)) if message == "Nome nao pode ser vazio")
        );
    }

    #[test]
    fn validate_todo_fields_rejects_empty_description() {
        let result = validate_todo_fields("Tarefa valida".to_string(), "   ".to_string());

        assert!(
            matches!(result, Err(ApiError::BadRequest(message)) if message == "Descricao nao pode ser vazia")
        );
    }

    #[test]
    fn validate_todo_fields_rejects_short_name() {
        let result = validate_todo_fields("Ab".to_string(), "Descricao valida".to_string());

        assert!(
            matches!(result, Err(ApiError::BadRequest(message)) if message == "O nome deve ter no minimo 3 caracteres")
        );
    }

    #[test]
    fn validate_todo_fields_trims_name_and_description() {
        let result = validate_todo_fields(
            "  Tarefa valida  ".to_string(),
            "  Descricao valida  ".to_string(),
        );

        let (name, description) = result.expect("validation should succeed");

        assert_eq!(name, "Tarefa valida");
        assert_eq!(description, "Descricao valida");
    }
}
