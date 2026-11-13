-- The recurrence runner's progress: every occurrence on or before this date
-- has been handled (fired, or deliberately not fired because the recurrence
-- was inactive or imported from a system that fired it already). NULL means
-- nothing handled yet, so the runner starts at first_date.
ALTER TABLE recurrences ADD COLUMN fired_through TEXT;
