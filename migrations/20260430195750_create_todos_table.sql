-- Add migration script here
CREATE TABLE todos (
    id INT AUTO_INCREMENT PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    description TEXT NOT NULL,
    finished BOOLEAN NOT NULL DEFAULT FALSE
);
