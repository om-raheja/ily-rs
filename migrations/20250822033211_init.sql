CREATE TABLE IF NOT EXISTS users (
    id SERIAL PRIMARY KEY,
    username VARCHAR(255) UNIQUE NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    created decimal default extract(epoch from now()),
    view_history BOOLEAN DEFAULT TRUE NOT NULL
);
CREATE TABLE IF NOT EXISTS messages (
    id SERIAL PRIMARY KEY,
    username VARCHAR(255) NOT NULL,
    message TEXT NOT NULL,
    sent_at DECIMAL DEFAULT EXTRACT(EPOCH FROM NOW())
);
