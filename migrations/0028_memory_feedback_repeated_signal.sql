-- Allow the 'repeated' feedback signal (7.103 wave 2: failure-lesson escalation).

ALTER TABLE memory_feedback_events
    DROP CONSTRAINT IF EXISTS memory_feedback_signal_check;

ALTER TABLE memory_feedback_events
    ADD CONSTRAINT memory_feedback_signal_check CHECK (
        signal IN ('used', 'helpful', 'harmful', 'corrected', 'rejected', 'idle_decay', 'repeated')
    );
