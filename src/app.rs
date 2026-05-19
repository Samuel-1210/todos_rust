use axum::{
    Router,
    routing::{get, post},
};

use crate::{
    state::AppState,
    todos::{create_todo, delete_todo, get_todo_by_id, get_todos, patch_todo, update_todo},
};

async fn health_check() -> &'static str {
    "API rodando"
}

pub(crate) fn build_app(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/todos", post(create_todo).get(get_todos))
        .route(
            "/todos/:id",
            get(get_todo_by_id)
                .put(update_todo)
                .patch(patch_todo)
                .delete(delete_todo),
        )
        .with_state(state)
}
