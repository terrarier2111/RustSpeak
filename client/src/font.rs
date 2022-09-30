/*
use std::mem::transmute;
use std::simd::{f32x4, StdFloat};
use crate::platform::{self, abs, atan2, f32x4, sqrt};
use crate::{Glyph, OutlineBounds};
use alloc::vec;
use alloc::vec::*;

#[derive(Copy, Clone, PartialEq, Debug)]
struct AABB {
    /// Coordinate of the left-most edge.
    xmin: f32,
    /// Coordinate of the right-most edge.
    xmax: f32,
    /// Coordinate of the bottom-most edge.
    ymin: f32,
    /// Coordinate of the top-most edge.
    ymax: f32,
}

impl Default for AABB {
    fn default() -> Self {
        AABB {
            xmin: 0.0,
            xmax: 0.0,
            ymin: 0.0,
            ymax: 0.0,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
struct CubeCurve {
    a: Point,
    b: Point,
    c: Point,
    d: Point,
}

impl CubeCurve {
    const fn new(a: Point, b: Point, c: Point, d: Point) -> CubeCurve {
        CubeCurve {
            a,
            b,
            c,
            d,
        }
    }

    fn scale(&self, scale: f32) -> CubeCurve {
        CubeCurve {
            a: self.a.scale(scale),
            b: self.b.scale(scale),
            c: self.c.scale(scale),
            d: self.d.scale(scale),
        }
    }

    fn is_flat(&self, threshold: f32) -> bool {
        let (d1, d2, d3, d4) = f32x4::from([
            self.a.distance_squared(self.b),
            self.b.distance_squared(self.c),
            self.c.distance_squared(self.d),
            self.a.distance_squared(self.d),
        ])
            .sqrt()
            .copied();
        (d1 + d2 + d3) < threshold * d4
    }

    fn split(&self) -> (CubeCurve, CubeCurve) {
        let q0 = self.a.midpoint(self.b);
        let q1 = self.b.midpoint(self.c);
        let q2 = self.c.midpoint(self.d);
        let r0 = q0.midpoint(q1);
        let r1 = q1.midpoint(q2);
        let s0 = r0.midpoint(r1);
        (CubeCurve::new(self.a, q0, r0, s0), CubeCurve::new(s0, r1, q2, self.d))
    }

    /// The point at time t in the curve.
    fn point(&self, t: f32) -> Point {
        let tm = 1.0 - t;
        let a = tm * tm * tm;
        let b = 3.0 * (tm * tm) * t;
        let c = 3.0 * tm * (t * t);
        let d = t * t * t;

        let x = a * self.a.x + b * self.b.x + c * self.c.x + d * self.d.x;
        let y = a * self.a.y + b * self.b.y + c * self.c.y + d * self.d.y;
        Point::new(x, y)
    }

    /// The slope of the tangent line at time t.
    fn slope(&self, t: f32) -> (f32, f32) {
        let tm = 1.0 - t;
        let a = 3.0 * (tm * tm);
        let b = 6.0 * tm * t;
        let c = 3.0 * (t * t);

        let x = a * (self.b.x - self.a.x) + b * (self.c.x - self.b.x) + c * (self.d.x - self.c.x);
        let y = a * (self.b.y - self.a.y) + b * (self.c.y - self.b.y) + c * (self.d.y - self.c.y);
        (x, y)
    }

    /// The angle of the tangent line at time t in rads.
    fn angle(&self, t: f32) -> f32 {
        let (x, y) = self.slope(t);
        abs(atan2(x, y))
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
struct QuadCurve {
    a: Point,
    b: Point,
    c: Point,
}

impl QuadCurve {
    fn new(a: Point, b: Point, c: Point) -> QuadCurve {
        QuadCurve {
            a,
            b,
            c,
        }
    }

    fn scale(&self, scale: f32) -> QuadCurve {
        QuadCurve {
            a: self.a.scale(scale),
            b: self.b.scale(scale),
            c: self.c.scale(scale),
        }
    }

    fn is_flat(&self, threshold: f32) -> bool {
        let (d1, d2, d3, _) = f32x4::from([
            self.a.distance_squared(self.b),
            self.b.distance_squared(self.c),
            self.a.distance_squared(self.c),
            1.0,
            ]
        )
            .sqrt()
            .copied();
        (d1 + d2) < threshold * d3
    }

    fn split(&self) -> (QuadCurve, QuadCurve) {
        let q0 = self.a.midpoint(self.b);
        let q1 = self.b.midpoint(self.c);
        let r0 = q0.midpoint(q1);
        (QuadCurve::new(self.a, q0, r0), QuadCurve::new(r0, q1, self.c))
    }

    /// The point at time t in the curve.
    fn point(&self, t: f32) -> Point {
        let tm = 1.0 - t;
        let a = tm * tm;
        let b = 2.0 * tm * t;
        let c = t * t;

        let x = a * self.a.x + b * self.b.x + c * self.c.x;
        let y = a * self.a.y + b * self.b.y + c * self.c.y;
        Point::new(x, y)
    }

    /// The slope of the tangent line at time t.
    fn slope(&self, t: f32) -> (f32, f32) {
        let tm = 1.0 - t;
        let a = 2.0 * tm;
        let b = 2.0 * t;

        let x = a * (self.b.x - self.a.x) + b * (self.c.x - self.b.x);
        let y = a * (self.b.y - self.a.y) + b * (self.c.y - self.b.y);
        (x, y)
    }

    /// The angle of the tangent line at time t in rads.
    fn angle(&self, t: f32) -> f32 {
        let (x, y) = self.slope(t);
        abs(atan2(x, y))
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Point {
    /// Absolute X coordinate.
    pub x: f32,
    /// Absolute Y coordinate.
    pub y: f32,
}

impl Default for Point {
    fn default() -> Self {
        Point {
            x: 0.0,
            y: 0.0,
        }
    }
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Point {
        Point {
            x,
            y,
        }
    }

    pub fn scale(&self, scale: f32) -> Point {
        Point {
            x: self.x * scale,
            y: self.y * scale,
        }
    }

    pub fn distance_squared(&self, other: Point) -> f32 {
        let x = self.x - other.x;
        let y = self.y - other.y;
        x * x + y * y
    }

    pub fn distance(&self, other: Point) -> f32 {
        let x = self.x - other.x;
        let y = self.y - other.y;
        sqrt(x * x + y * y)
    }

    pub fn midpoint(&self, other: Point) -> Point {
        Point {
            x: (self.x + other.x) / 2.0,
            y: (self.y + other.y) / 2.0,
        }
    }
}

#[derive(Copy, Clone)]
pub struct Line {
    /// X0, Y0, X1, Y1.
    pub coords: f32x4,
    /// start_x_nudge, start_y_nudge, end_x_nudge, end_y_nudge.
    pub nudge: f32x4,
    /// x_first_adj, y_first_adj, none, none.
    pub adjustment: f32x4,
    /// tdx, tdy, dx, dy.
    pub params: f32x4,
}

impl Line {
    pub fn new(start: Point, end: Point) -> Line {
        // Floor adjustment and nudge: 0.0, 0
        // Ceil adjustment and nudge: 1.0, 1
        const FLOOR_NUDGE: u32 = 0;
        const CEIL_NUDGE: u32 = 1;

        let (x_start_nudge, x_first_adj) = if end.x >= start.x {
            (FLOOR_NUDGE, 1.0)
        } else {
            (CEIL_NUDGE, 0.0)
        };
        let (y_start_nudge, y_first_adj) = if end.y >= start.y {
            (FLOOR_NUDGE, 1.0)
        } else {
            (CEIL_NUDGE, 0.0)
        };

        let x_end_nudge = if end.x > start.x {
            CEIL_NUDGE
        } else {
            FLOOR_NUDGE
        };
        let y_end_nudge = if end.y > start.y {
            CEIL_NUDGE
        } else {
            FLOOR_NUDGE
        };

        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let tdx = if dx == 0.0 {
            f32::MAX
        } else {
            1.0 / dx
        };
        let tdy = 1.0 / dy;

        Line {
            coords: f32x4::from([start.x, start.y, end.x, end.y]),
            nudge: f32x4::from([unsafe { transmute(x_start_nudge) }, unsafe { transmute(y_start_nudge) },
                unsafe { transmute(x_end_nudge) }, unsafe { transmute(y_end_nudge) }]),
            adjustment: f32x4::from([x_first_adj, y_first_adj, 0.0, 0.0]),
            params: f32x4::from([tdx, tdy, dx, dy]),
        }
    }

    fn reposition(&mut self, bounds: AABB, reverse: bool) {
        let (mut x0, mut y0, mut x1, mut y1) = if !reverse {
            self.coords.copied()
        } else {
            let (x0, y0, x1, y1) = self.coords.copied();
            (x1, y1, x0, y0)
        };

        x0 -= bounds.xmin;
        y0 -= bounds.ymax;
        y0 = abs(y0);

        x1 -= bounds.xmin;
        y1 -= bounds.ymax;
        y1 = abs(y1);

        *self = Self::new(Point::new(x0, y0), Point::new(x1, y1));
    }
}

#[derive(Clone)]
pub struct Geometry {
    v_lines: Vec<Line>,
    m_lines: Vec<Line>,
    effective_bounds: AABB,
    start_point: Point,
    previous_point: Point,
    area: f32,
    reverse_points: bool,
    max_area: f32,
}

struct Segment {
    a: Point,
    at: f32,
    c: Point,
    ct: f32,
}

impl Segment {
    const fn new(a: Point, at: f32, c: Point, ct: f32) -> Segment {
        Segment {
            a,
            at,
            c,
            ct,
        }
    }
}

impl ttf_parser::OutlineBuilder for Geometry {
    fn move_to(&mut self, x0: f32, y0: f32) {
        let next_point = Point::new(x0, y0);
        self.start_point = next_point;
        self.previous_point = next_point;
    }

    fn line_to(&mut self, x0: f32, y0: f32) {
        let next_point = Point::new(x0, y0);
        self.push(self.previous_point, next_point);
        self.previous_point = next_point;
    }

    fn quad_to(&mut self, x0: f32, y0: f32, x1: f32, y1: f32) {
        let control_point = Point::new(x0, y0);
        let next_point = Point::new(x1, y1);

        let curve = QuadCurve::new(self.previous_point, control_point, next_point);
        let mut stack = vec![Segment::new(self.previous_point, 0.0, next_point, 1.0)];
        while let Some(seg) = stack.pop() {
            let bt = (seg.at + seg.ct) * 0.5;
            let b = curve.point(bt);
            // This is twice the triangle area
            let area = (b.x - seg.a.x) * (seg.c.y - seg.a.y) - (seg.c.x - seg.a.x) * (b.y - seg.a.y);
            if abs(area) > self.max_area {
                stack.push(Segment::new(seg.a, seg.at, b, bt));
                stack.push(Segment::new(b, bt, seg.c, seg.ct));
            } else {
                self.push(seg.a, seg.c);
            }
        }

        self.previous_point = next_point;
    }

    fn curve_to(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, x2: f32, y2: f32) {
        let first_control = Point::new(x0, y0);
        let second_control = Point::new(x1, y1);
        let next_point = Point::new(x2, y2);

        let curve = CubeCurve::new(self.previous_point, first_control, second_control, next_point);
        let mut stack = vec![Segment::new(self.previous_point, 0.0, next_point, 1.0)];
        while let Some(seg) = stack.pop() {
            let bt = (seg.at + seg.ct) * 0.5;
            let b = curve.point(bt);
            // This is twice the triangle area
            let area = (b.x - seg.a.x) * (seg.c.y - seg.a.y) - (seg.c.x - seg.a.x) * (b.y - seg.a.y);
            if abs(area) > self.max_area {
                stack.push(Segment::new(seg.a, seg.at, b, bt));
                stack.push(Segment::new(b, bt, seg.c, seg.ct));
            } else {
                self.push(seg.a, seg.c);
            }
        }
        self.previous_point = next_point;
    }

    fn close(&mut self) {
        if self.start_point != self.previous_point {
            self.push(self.previous_point, self.start_point);
        }
        self.previous_point = self.start_point;
    }
}

impl Geometry {
    // Artisanal bespoke hand carved curves
    pub fn new(scale: f32, units_per_em: f32) -> Geometry {
        const ERROR_THRESHOLD: f32 = 3.0; // In pixels.
        let max_area = ERROR_THRESHOLD * 2.0 * (units_per_em / scale);

        Geometry {
            v_lines: Vec::new(),
            m_lines: Vec::new(),
            effective_bounds: AABB {
                xmin: core::f32::MAX,
                xmax: core::f32::MIN,
                ymin: core::f32::MAX,
                ymax: core::f32::MIN,
            },
            start_point: Point::default(),
            previous_point: Point::default(),
            area: 0.0,
            reverse_points: false,
            max_area,
        }
    }

    fn push(&mut self, start: Point, end: Point) {
        // We're using to_bits here because we only care if they're _exactly_ the same.
        if start.y.to_bits() != end.y.to_bits() {
            self.area += (end.y - start.y) * (end.x + start.x);
            if start.x.to_bits() == end.x.to_bits() {
                self.v_lines.push(Line::new(start, end));
            } else {
                self.m_lines.push(Line::new(start, end));
            }
            Self::recalculate_bounds(&mut self.effective_bounds, start.x, start.y);
            Self::recalculate_bounds(&mut self.effective_bounds, end.x, end.y);
        }
    }

    pub(crate) fn finalize(mut self, glyph: &mut Glyph) {
        if self.v_lines.is_empty() && self.m_lines.is_empty() {
            self.effective_bounds = AABB::default();
        } else {
            self.reverse_points = self.area > 0.0;
            for line in self.v_lines.iter_mut().chain(self.m_lines.iter_mut()) {
                line.reposition(self.effective_bounds, self.reverse_points);
            }
            self.v_lines.shrink_to_fit();
            self.m_lines.shrink_to_fit();
        }
        glyph.v_lines = self.v_lines;
        glyph.m_lines = self.m_lines;
        glyph.bounds = OutlineBounds {
            xmin: self.effective_bounds.xmin,
            ymin: self.effective_bounds.ymin,
            width: self.effective_bounds.xmax - self.effective_bounds.xmin,
            height: self.effective_bounds.ymax - self.effective_bounds.ymin,
        };
    }

    fn recalculate_bounds(bounds: &mut AABB, x: f32, y: f32) {
        if x < bounds.xmin {
            bounds.xmin = x;
        }
        if x > bounds.xmax {
            bounds.xmax = x;
        }
        if y < bounds.ymin {
            bounds.ymin = y;
        }
        if y > bounds.ymax {
            bounds.ymax = y;
        }
    }
}

/// Defines the bounds for a glyph's outline in subpixels. A glyph's outline is always contained in
/// its bitmap.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct OutlineBounds {
    /// Subpixel offset of the left-most edge of the glyph's outline.
    pub xmin: f32,
    /// Subpixel offset of the bottom-most edge of the glyph's outline.
    pub ymin: f32,
    /// The width of the outline in subpixels.
    pub width: f32,
    /// The height of the outline in subpixels.
    pub height: f32,
}

/// Sets the high bit 0x80000000 on a float.
#[inline(always)]
fn abs(value: f32) -> f32 {
    f32::from_bits(value.to_bits() & 0x7fffffff)
}*/

/*
pub fn character_texture(&mut self, c: char) -> render::Texture {
        let raw = c as u32;
        let page = raw >> 8;
        // Lazy load fonts to size memory
        if self.font_pages[page as usize].is_none() {
            let name = if page == 0 {
                "font/ascii".to_owned()
            } else {
                format!("font/unicode_page_{:02X}", page)
            };
            let textures = self.textures.clone();
            self.font_pages[page as usize] = Some(render::Renderer::get_texture(&textures, &name));
        }
        let p = self.font_pages[page as usize].clone().unwrap();

        let raw = if page == 0 {
            (*self.char_map.get(&c).unwrap_or(&c)) as u32
        } else {
            raw
        };
        let ch = raw & 0xFF;
        let cx = ch & 0xF;
        let cy = ch >> 4;
        let info = self.font_character_info[raw as usize];
        if page == 0 {
            let sw = (self.page_width / 16.0) as u32;
            let sh = (self.page_height / 16.0) as u32;
            return p.relative(
                (cx * sw + info.0 as u32) as f32 / (self.page_width as f32),
                (cy * sh) as f32 / (self.page_height as f32),
                (info.1 - info.0) as f32 / (self.page_width as f32),
                (sh as f32) / (self.page_height as f32),
            );
        }
        p.relative(
            (cx * 16 + info.0 as u32) as f32 / 256.0,
            (cy * 16) as f32 / 256.0,
            (info.1 - info.0) as f32 / 256.0,
            16.0 / 256.0,
        )
    }

    pub fn size_of_string(&self, val: &str) -> f64 {
        let mut size = 0.0;
        for c in val.chars() {
            size += self.size_of_char(c) + 2.0;
        }
        size - 2.0
    }

    pub fn size_of_char(&self, c: char) -> f64 {
        if c == ' ' {
            return 4.0;
        }
        let r = c as u32;
        if r >> 8 == 0 {
            let r = (*self.char_map.get(&c).unwrap_or(&c)) as u32;
            let info = self.font_character_info[r as usize];
            let sw = self.page_width / 16.0;
            return (((info.1 - info.0) as f64) / sw) * 16.0;
        }
        let info = self.font_character_info[c as usize];
        (info.1 - info.0) as f64
    }

    fn load_font(&mut self) {
        for page in &mut self.font_pages {
            *page = None;
        }
        let res = self.resources.read();
        if let Some(mut info) = res.open("minecraft", "font/glyph_sizes.bin") {
            let mut data = Vec::with_capacity(0x10000);
            info.read_to_end(&mut data).unwrap();
            for (i, info) in self.font_character_info.iter_mut().enumerate() {
                // Top nibble - start position
                // Bottom nibble - end position
                info.0 = (data[i] >> 4) as i32;
                info.1 = (data[i] & 0xF) as i32 + 1;
            }
        }
        if let Some(mut val) = res.open("minecraft", "textures/font/ascii.png") {
            let mut data = Vec::new();
            val.read_to_end(&mut data).unwrap();
            if let Ok(img) = image::load_from_memory(&data) {
                let (width, height) = img.dimensions();
                self.page_width = width as f64;
                self.page_height = height as f64;
                let sw = width / 16;
                let sh = height / 16;
                for i in 0..256 {
                    let cx = (i & 0xF) * sw;
                    let cy = (i >> 4) * sh;
                    let mut start = true;
                    'x_loop: for x in 0..sw {
                        for y in 0..sh {
                            let a = img.get_pixel(cx + x, cy + y).0[3];
                            if start && a != 0 {
                                self.font_character_info[i as usize].0 = x as i32;
                                start = false;
                                continue 'x_loop;
                            } else if !start && a != 0 {
                                continue 'x_loop;
                            }
                        }
                        if !start {
                            self.font_character_info[i as usize].1 = x as i32;
                            break;
                        }
                    }
                }
            }
        }
    }

*/
