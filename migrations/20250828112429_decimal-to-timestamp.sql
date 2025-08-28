ALTER TABLE users
    ALTER COLUMN created DROP DEFAULT;
ALTER TABLE users
    ALTER COLUMN created TYPE TIMESTAMP USING to_timestamp(created);
ALTER TABLE users
    ALTER COLUMN created SET DEFAULT now();

ALTER TABLE messages
    ALTER COLUMN sent_at DROP DEFAULT;
ALTER TABLE messages
    ALTER COLUMN sent_at TYPE TIMESTAMP USING to_timestamp(sent_at);
ALTER TABLE messages
    ALTER COLUMN sent_at SET DEFAULT now();
