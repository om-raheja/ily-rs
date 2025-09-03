-- Migration to extract text from JSON-formatted messages
-- NOTE THAT IF YOUR MESSAGES ARE MALICIOUSLY FORMED WITH '{}', YOURE COOKED
UPDATE messages 
SET message = (
    CASE 
        WHEN message LIKE '{%}' AND message::jsonb ? 'text' THEN message::jsonb ->> 'text'
        ELSE message 
    END
)
WHERE message IS NOT NULL;
