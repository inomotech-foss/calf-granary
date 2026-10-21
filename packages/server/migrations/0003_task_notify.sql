CREATE FUNCTION notify_task() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('task', '');
    RETURN NULL;
END
$$;

CREATE TRIGGER task_pending
AFTER INSERT OR UPDATE OF state, due_at ON task
FOR EACH ROW WHEN (NEW.state = 'pending')
EXECUTE FUNCTION notify_task();
