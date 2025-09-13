ALTER TABLE users
    ADD COLUMN IF NOT EXISTS channels TEXT[] DEFAULT ARRAY['main'] NOT NULL;

ALTER TABLE messages
    ADD COLUMN IF NOT EXISTS channel VARCHAR(255) DEFAULT 'main' NOT NULL;  

ALTER TABLE users
    ALTER COLUMN view_history DROP DEFAULT,
    ALTER COLUMN view_history TYPE boolean[] USING ARRAY[view_history],
    ALTER COLUMN view_history SET DEFAULT ARRAY[true];
