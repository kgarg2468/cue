PASS

No MAJOR or MINOR findings. The ceiling probe covers every value `now_milliseconds()` can persist, and the regression test synchronously re-narrows the row inside every sweep iteration before issuing the update.
