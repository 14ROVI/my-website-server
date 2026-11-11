use serde::Deserialize;
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform,
};

pub const WIDTH: u32 = 640;
pub const HEIGHT: u32 = 400;

#[derive(Clone, Copy, Deserialize)]
pub enum Tool {
    Pencil,
    Brush,
    Eraser,
    Fill,
    Line,
    Rectangle,
    Ellipse,
    Clear,
}

#[derive(Deserialize)]
pub struct Edit {
    pub tool: Tool,
    pub colour: String,
    pub size: f32,
    pub points: Vec<[f32; 2]>,
}

impl Edit {
    pub fn valid(&self) -> bool {
        let count = match self.tool {
            Tool::Clear | Tool::Fill => self.points.len() == 1,
            Tool::Line | Tool::Rectangle | Tool::Ellipse => self.points.len() == 2,
            _ => (1..=2048).contains(&self.points.len()),
        };
        count
            && self
                .colour
                .strip_prefix('#')
                .is_some_and(|hex| hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
            && (1.0..=32.0).contains(&self.size)
            && self
                .points
                .iter()
                .all(|[x, y]| (0.0..WIDTH as f32).contains(x) && (0.0..HEIGHT as f32).contains(y))
    }

    pub fn apply(&self, canvas: &mut Pixmap) {
        let [x, y] = self.points[0];
        let [end_x, end_y] = *self.points.last().unwrap();
        let rgb = u32::from_str_radix(&self.colour[1..], 16).unwrap();
        let (r, g, b) = if matches!(self.tool, Tool::Eraser) {
            (255, 255, 255)
        } else {
            ((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
        };
        let colour = Color::from_rgba8(r, g, b, 255);
        match self.tool {
            Tool::Clear => {
                canvas.fill(Color::WHITE);
                return;
            }
            Tool::Fill => {
                flood_fill(
                    canvas.data_mut(),
                    WIDTH as usize,
                    HEIGHT as usize,
                    x as usize,
                    y as usize,
                    [r, g, b, 255],
                );
                return;
            }
            _ => {}
        }

        let mut paint = Paint::default();
        paint.set_color(colour);
        let width = if matches!(self.tool, Tool::Pencil) {
            1.0
        } else {
            self.size
        };
        let mut path = PathBuilder::new();
        match self.tool {
            Tool::Rectangle | Tool::Ellipse => {
                let Some(rect) =
                    Rect::from_ltrb(x.min(end_x), y.min(end_y), x.max(end_x), y.max(end_y))
                else {
                    return;
                };
                if matches!(self.tool, Tool::Rectangle) {
                    path.push_rect(rect);
                } else {
                    path.push_oval(rect);
                }
            }
            _ if self.points.iter().all(|point| *point == [x, y]) => {
                path.push_circle(x, y, width / 2.0);
                if let Some(path) = path.finish() {
                    canvas.fill_path(
                        &path,
                        &paint,
                        FillRule::Winding,
                        Transform::identity(),
                        None,
                    );
                }
                return;
            }
            _ => {
                path.move_to(x, y);
                for &[x, y] in &self.points[1..] {
                    path.line_to(x, y);
                }
            }
        }
        if let Some(path) = path.finish() {
            let stroke = Stroke {
                width,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                ..Stroke::default()
            };
            canvas.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }
}

pub fn blank_canvas() -> Pixmap {
    let mut canvas = Pixmap::new(WIDTH, HEIGHT).unwrap();
    canvas.fill(Color::WHITE);
    canvas
}
fn flood_fill(pixels: &mut [u8], width: usize, height: usize, x: usize, y: usize, colour: [u8; 4]) {
    let offset = (y * width + x) * 4;
    let target: [u8; 4] = pixels[offset..offset + 4].try_into().unwrap();
    if target == colour {
        return;
    }
    let mut pending = vec![(x, y)];
    pixels[offset..offset + 4].copy_from_slice(&colour);
    while let Some((x, y)) = pending.pop() {
        for (nx, ny) in [
            (x.wrapping_sub(1), y),
            (x + 1, y),
            (x, y.wrapping_sub(1)),
            (x, y + 1),
        ] {
            if nx >= width || ny >= height {
                continue;
            }
            let i = (ny * width + nx) * 4;
            if pixels[i..i + 4] == target {
                pixels[i..i + 4].copy_from_slice(&colour);
                pending.push((nx, ny));
            }
        }
    }
}
