use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, MySql, MySqlPool, QueryBuilder};

use crate::{
    error::{ApiError, ApiResult},
    state::AppState,
};

const DEFAULT_LIMIT: i64 = 20;
const MAX_LIMIT: i64 = 100;
const DEFAULT_PAGE: i64 = 1;

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

fn validate_single_todo_field(name: String) -> ApiResult<String> {
    let name = name.trim().to_owned();

    if name.is_empty() {
        return Err(ApiError::BadRequest("Nome nao pode ser vazio".to_string()));
    }

    if name.len() < 3 {
        return Err(ApiError::BadRequest(
            "O nome deve ter no minimo 3 caracteres".to_string(),
        ));
    }

    Ok(name)
}

fn validate_patch_todo_fields(payload: PatchTodo) -> ApiResult<NormalizedPatchTodo> {
    if payload.name.is_none() && payload.description.is_none() && payload.finished.is_none() {
        return Err(ApiError::BadRequest(
            "Pelo menos um campo deve ser informado".to_string(),
        ));
    }

    let name = match payload.name {
        Some(name) => Some(validate_single_todo_field(name)?),
        None => None,
    };

    let description = match payload.description {
        Some(description) => {
            let description = description.trim().to_owned();
            if description.is_empty() {
                return Err(ApiError::BadRequest(
                    "Descricao nao pode ser vazia".to_string(),
                ));
            }
            Some(description)
        }
        None => None,
    };

    Ok(NormalizedPatchTodo {
        name,
        description,
        finished: payload.finished,
    })
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub(crate) struct Todo {
    id: i32,
    name: String,
    description: String,
    finished: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub(crate) struct TodoListResponse {
    items: Vec<Todo>,
    page: i64,
    limit: i64,
    total: i64,
    prev: Option<i64>,
    next: Option<i64>,
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

#[derive(Debug, Deserialize)]
pub(crate) struct PatchTodo {
    name: Option<String>,
    description: Option<String>,
    finished: Option<bool>,
}

#[derive(Debug)]
struct NormalizedPatchTodo {
    name: Option<String>,
    description: Option<String>,
    finished: Option<bool>,
}

#[derive(Debug)]
enum TodoSortField {
    Id,
    Name,
    Description,
    Finished,
    CreatedAt,
    UpdatedAt,
}

impl TodoSortField {
    fn as_sql(&self) -> &'static str {
        match self {
            Self::Id => "id",
            Self::Name => "name",
            Self::Description => "description",
            Self::Finished => "finished",
            Self::CreatedAt => "created_at",
            Self::UpdatedAt => "updated_at",
        }
    }
}

#[derive(Debug)]
enum SortDirection {
    Asc,
    Desc,
}

impl SortDirection {
    fn as_sql(&self) -> &'static str {
        match self {
            Self::Asc => "ASC",
            Self::Desc => "DESC",
        }
    }
}

#[derive(Debug)]
struct ValidatedTodoQuery {
    search: Option<String>,
    finished: Option<bool>,
    limit: i64,
    page: i64,
    offset: i64,
    sort_by: TodoSortField,
    order_by: SortDirection,
}

fn validate_todo_query_params(params: TodoQueryParams) -> ApiResult<ValidatedTodoQuery> {
    let limit = params.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(ApiError::BadRequest(format!(
            "limit deve estar entre 1 e {MAX_LIMIT}"
        )));
    }

    let page = params.page.unwrap_or(DEFAULT_PAGE);
    if page < DEFAULT_PAGE {
        return Err(ApiError::BadRequest(
            "page deve ser maior ou igual a 1".to_string(),
        ));
    }

    let sort_by = match params.sort_by.as_deref().map(str::trim) {
        None | Some("") | Some("id") => TodoSortField::Id,
        Some("name") => TodoSortField::Name,
        Some("description") => TodoSortField::Description,
        Some("finished") => TodoSortField::Finished,
        Some("created_at") => TodoSortField::CreatedAt,
        Some("updated_at") => TodoSortField::UpdatedAt,
        Some(_) => {
            return Err(ApiError::BadRequest(
                "sort_by deve ser um de: id, name, description, finished, created_at, updated_at"
                    .to_string(),
            ));
        }
    };

    let order_by = match params.order_by.as_deref().map(str::trim) {
        None | Some("") => SortDirection::Asc,
        Some(value) if value.eq_ignore_ascii_case("asc") => SortDirection::Asc,
        Some(value) if value.eq_ignore_ascii_case("desc") => SortDirection::Desc,
        Some(_) => {
            return Err(ApiError::BadRequest(
                "order_by deve ser asc ou desc".to_string(),
            ));
        }
    };

    let offset = page
        .checked_sub(1)
        .and_then(|page_index| page_index.checked_mul(limit))
        .ok_or_else(|| ApiError::BadRequest("Combinacao de page e limit invalida".to_string()))?;

    Ok(ValidatedTodoQuery {
        search: params.search,
        finished: params.finished,
        limit,
        page,
        offset,
        sort_by,
        order_by,
    })
}

fn validate_todo_timestamps(todo: Todo) -> ApiResult<Todo> {
    if todo.updated_at < todo.created_at {
        return Err(ApiError::Database(sqlx::Error::Protocol(
            "updated_at veio anterior a created_at".to_string(),
        )));
    }

    Ok(todo)
}

async fn fetch_todo_by_id(pool: &MySqlPool, id: i32) -> ApiResult<Todo> {
    let todo = sqlx::query_as::<_, Todo>(
        r#"
        SELECT id, name, description, finished, created_at, updated_at
        FROM todos
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(ApiError::Database)?
    .ok_or_else(|| ApiError::NotFound(format!("Todo {id} nao encontrado")))?;

    validate_todo_timestamps(todo)
}

fn apply_todo_filters(query_builder: &mut QueryBuilder<'_, MySql>, params: &ValidatedTodoQuery) {
    let mut has_where = false;

    if let Some(finished) = params.finished {
        query_builder.push(" WHERE finished = ").push_bind(finished);
        has_where = true;
    }

    if let Some(search) = params.search.as_deref().map(str::trim) {
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

    let result = sqlx::query(
        r#"
        INSERT INTO todos (name, description, finished)
        VALUES (?, ?, ?)
        "#,
    )
    .bind(&name)
    .bind(&description)
    .bind(finished)
    .execute(&state.db)
    .await
    .map_err(ApiError::Database)?;

    let todo = fetch_todo_by_id(&state.db, result.last_insert_id() as i32).await?;

    Ok((StatusCode::CREATED, Json(todo)))
}

pub(crate) async fn get_todos(
    State(state): State<AppState>,
    Query(params): Query<TodoQueryParams>,
) -> ApiResult<Json<TodoListResponse>> {
    let params = validate_todo_query_params(params)?;

    let mut count_builder = QueryBuilder::<MySql>::new("SELECT COUNT(*) FROM todos");
    apply_todo_filters(&mut count_builder, &params);

    let total = count_builder
        .build_query_scalar::<i64>()
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::Database)?;

    let mut items_builder = QueryBuilder::<MySql>::new(
        "SELECT id, name, description, finished, created_at, updated_at FROM todos",
    );
    apply_todo_filters(&mut items_builder, &params);
    items_builder
        .push(" ORDER BY ")
        .push(params.sort_by.as_sql())
        .push(" ")
        .push(params.order_by.as_sql())
        .push(" LIMIT ")
        .push_bind(params.limit)
        .push(" OFFSET ")
        .push_bind(params.offset);

    let items = items_builder
        .build_query_as::<Todo>()
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::Database)?
        .into_iter()
        .map(validate_todo_timestamps)
        .collect::<ApiResult<Vec<_>>>()?;

    let prev = (params.page > DEFAULT_PAGE).then_some(params.page - 1);
    let next = (params.offset + params.limit < total).then_some(params.page + 1);

    Ok(Json(TodoListResponse {
        items,
        page: params.page,
        limit: params.limit,
        total,
        prev,
        next,
    }))
}

pub(crate) async fn get_todo_by_id(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> ApiResult<Json<Todo>> {
    let todo = fetch_todo_by_id(&state.db, id).await?;

    Ok(Json(todo))
}

pub(crate) async fn update_todo(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Json(payload): Json<UpdateTodo>,
) -> ApiResult<Json<Todo>> {
    let UpdateTodo {
        name,
        description,
        finished,
    } = payload;

    let (name, description) = validate_todo_fields(name, description)?;

    let result = sqlx::query(
        r#"
        UPDATE todos
        SET name = ?, description = ?, finished = ?
        WHERE id = ?
        "#,
    )
    .bind(&name)
    .bind(&description)
    .bind(finished)
    .bind(id)
    .execute(&state.db)
    .await
    .map_err(ApiError::Database)?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound(format!("Todo {id} nao encontrado")));
    }

    let todo = fetch_todo_by_id(&state.db, id).await?;

    Ok(Json(todo))
}

pub(crate) async fn patch_todo(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Json(payload): Json<PatchTodo>,
) -> ApiResult<Json<Todo>> {
    let payload = validate_patch_todo_fields(payload)?;
    let current_todo = fetch_todo_by_id(&state.db, id).await?;

    let name = payload.name.unwrap_or(current_todo.name);
    let description = payload.description.unwrap_or(current_todo.description);
    let finished = payload.finished.unwrap_or(current_todo.finished);

    sqlx::query(
        r#"
        UPDATE todos
        SET name = ?, description = ?, finished = ?
        WHERE id = ?
        "#,
    )
    .bind(&name)
    .bind(&description)
    .bind(finished)
    .bind(id)
    .execute(&state.db)
    .await
    .map_err(ApiError::Database)?;

    let todo = fetch_todo_by_id(&state.db, id).await?;

    Ok(Json(todo))
}

pub(crate) async fn delete_todo(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> ApiResult<StatusCode> {
    let result = sqlx::query(
        r#"
        DELETE FROM todos
        WHERE id = ?
        "#,
    )
    .bind(id)
    .execute(&state.db)
    .await
    .map_err(ApiError::Database)?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound(format!("Todo {id} nao encontrado")));
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

    #[test]
    fn validate_todo_query_params_rejects_invalid_limit() {
        let result = validate_todo_query_params(TodoQueryParams {
            limit: Some(0),
            ..TodoQueryParams::default()
        });

        assert!(
            matches!(result, Err(ApiError::BadRequest(message)) if message == "limit deve estar entre 1 e 100")
        );
    }

    #[test]
    fn validate_todo_query_params_rejects_invalid_sort_by() {
        let result = validate_todo_query_params(TodoQueryParams {
            sort_by: Some("priority".to_string()),
            ..TodoQueryParams::default()
        });

        assert!(
            matches!(result, Err(ApiError::BadRequest(message)) if message == "sort_by deve ser um de: id, name, description, finished, created_at, updated_at")
        );
    }

    #[test]
    fn validate_todo_query_params_builds_offset() {
        let result = validate_todo_query_params(TodoQueryParams {
            page: Some(3),
            limit: Some(10),
            ..TodoQueryParams::default()
        })
        .expect("query params should be valid");

        assert_eq!(result.offset, 20);
        assert_eq!(result.page, 3);
        assert_eq!(result.limit, 10);
    }
}
