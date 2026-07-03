CREATE TABLE todo_events (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    todo_id INT NOT NULL,
    event_type VARCHAR(50) NOT NULL,
    payload TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    INDEX idx_todo_events_todo_id_created_at (todo_id, created_at),

    CONSTRAINT fk_todo_events_todo_id
        FOREIGN KEY (todo_id)
        REFERENCES todos(id)
);
