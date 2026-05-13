use sqlx::MySqlPool;

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) db: MySqlPool,
}
