CREATE TABLE paint_canvas (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    pixels BLOB NOT NULL CHECK (length(pixels) = 640 * 400 * 4)
);
