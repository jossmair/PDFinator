//! Native vector drawings, and conservative editing of existing straight strokes.
use crate::document::{Editor, Result, guarded};
use pdfcraft_content::{Matrix, Op, Pieces};
use pdfcraft_cos::{Dict, Document, ObjRef, Object, PdfString, SaveOptions, Stream};
use pdfcraft_render::PageInfo;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Kind {
    Line,
    Circle,
    Rectangle,
    Table,
    FilledRectangle,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum StrokeStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
    DashDot,
    LongDash,
    Double,
    Custom {
        pattern: Vec<f64>,
        phase: f64,
        cap: u8,
    },
}
impl StrokeStyle {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Solid => "Continu",
            Self::Dashed => "Tirets",
            Self::Dotted => "Pointillés",
            Self::DashDot => "Tirets et points",
            Self::LongDash => "Tirets longs",
            Self::Double => "Double",
            Self::Custom { .. } => "Style d’origine",
        }
    }
    pub fn pattern(&self, width: f64) -> (Vec<f64>, f64, u8) {
        match self {
            Self::Solid | Self::Double => (Vec::new(), 0., 0),
            Self::Dashed => (vec![5. * width, 3. * width], 0., 0),
            Self::Dotted => (vec![0., 3. * width], 0., 1),
            Self::DashDot => (vec![6. * width, 2.5 * width, 0., 2.5 * width], 0., 1),
            Self::LongDash => (vec![9. * width, 3. * width], 0., 0),
            Self::Custom {
                pattern,
                phase,
                cap,
            } => (pattern.clone(), *phase, *cap),
        }
    }
    fn from_pdf(pattern: &[f64], phase: f64, cap: u8, scale: f64) -> Self {
        if pattern.is_empty() && cap == 0 {
            Self::Solid
        } else {
            Self::Custom {
                pattern: pattern.iter().map(|v| v * scale).collect(),
                phase: phase * scale,
                cap,
            }
        }
    }
    fn valid(&self) -> bool {
        match self {
            Self::Custom {
                pattern,
                phase,
                cap,
            } => {
                pattern.len() <= 32
                    && *cap <= 2
                    && phase.is_finite()
                    && phase.abs() < 10_000_000.
                    && pattern
                        .iter()
                        .all(|v| v.is_finite() && (0.0..10_000_000.).contains(v))
                    && (pattern.is_empty() || pattern.iter().sum::<f64>() > 0.)
            }
            _ => true,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Drawing {
    pub kind: Kind,
    pub a: [f64; 2],
    pub u: [f64; 2],
    pub v: [f64; 2],
    pub width: f64,
    pub color: [f64; 3],
    #[serde(default)]
    pub style: StrokeStyle,
    pub rows: u16,
    pub columns: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Owned(ObjRef),
    Path(usize, usize),
}
#[derive(Clone, Debug)]
pub struct Item {
    pub target: Target,
    pub drawing: Drawing,
}
impl Drawing {
    pub fn valid(&self) -> bool {
        self.a
            .iter()
            .chain(&self.u)
            .chain(&self.v)
            .all(|v| v.is_finite() && v.abs() < 10_000_000.)
            && self.width.is_finite()
            && (0.05..=500.).contains(&self.width)
            && self
                .color
                .iter()
                .all(|c| c.is_finite() && (0.0..=1.).contains(c))
            && (1..=100).contains(&self.rows)
            && (1..=100).contains(&self.columns)
            && self.style.valid()
            && (self.style != StrokeStyle::Double || self.kind == Kind::Line)
    }
    pub fn at(&self, x: f64, y: f64) -> [f64; 2] {
        [
            self.a[0] + self.u[0] * x + self.v[0] * y,
            self.a[1] + self.u[1] * x + self.v[1] * y,
        ]
    }
    #[allow(clippy::too_many_arguments)]
    pub fn from_view(
        p: &PageInfo,
        kind: Kind,
        a: [f32; 2],
        b: [f32; 2],
        width: f64,
        color: [f64; 3],
        rows: u16,
        columns: u16,
    ) -> Self {
        let origin = p.view_to_user(a[0], a[1]).map(f64::from);
        let x = p.view_to_user(b[0], a[1]).map(f64::from);
        let y = p.view_to_user(a[0], b[1]).map(f64::from);
        let end = p.view_to_user(b[0], b[1]).map(f64::from);
        let one = p.view_to_user(a[0] + 1., a[1]);
        Self {
            kind,
            a: origin,
            u: if kind == Kind::Line {
                [end[0] - origin[0], end[1] - origin[1]]
            } else {
                [x[0] - origin[0], x[1] - origin[1]]
            },
            v: if kind == Kind::Line {
                [0.; 2]
            } else {
                [y[0] - origin[0], y[1] - origin[1]]
            },
            width: width * (one[0] as f64 - origin[0]).hypot(one[1] as f64 - origin[1]),
            color,
            style: StrokeStyle::Solid,
            rows,
            columns,
        }
    }
    pub fn view_points(&self, p: &PageInfo) -> Vec<[f32; 2]> {
        let points = if self.kind == Kind::Line {
            vec![self.at(0., 0.), self.at(1., 0.)]
        } else {
            vec![
                self.at(0., 0.),
                self.at(1., 0.),
                self.at(1., 1.),
                self.at(0., 1.),
            ]
        };
        points
            .into_iter()
            .map(|a| p.user_to_view(a[0] as f32, a[1] as f32))
            .collect()
    }
    pub fn view_width(&self, p: &PageInfo) -> f64 {
        let a = p.user_to_view(self.a[0] as f32, self.a[1] as f32);
        let b = p.user_to_view((self.a[0] + self.width) as f32, self.a[1] as f32);
        (b[0] - a[0]).hypot(b[1] - a[1]) as f64
    }
    pub fn translate(&mut self, delta: [f64; 2]) {
        self.a[0] += delta[0];
        self.a[1] += delta[1];
    }
    pub fn with_style(mut self, style: StrokeStyle) -> Self {
        self.style = style;
        self
    }
    pub fn ops(&self) -> Vec<Op> {
        let n = pdfcraft_content::num;
        let point = |name: &str, x: f64, y: f64| Op::new(name, self.at(x, y).map(n).to_vec());
        let (pattern, phase, cap) = self.style.pattern(self.width);
        let double = self.style == StrokeStyle::Double;
        let mut out = vec![
            Op::new("q", vec![]),
            Op::new(
                "w",
                vec![n(if double { self.width / 3. } else { self.width })],
            ),
            Op::new(
                if self.kind == Kind::FilledRectangle {
                    "rg"
                } else {
                    "RG"
                },
                self.color.map(n).to_vec(),
            ),
            Op::new(
                "d",
                vec![
                    Object::Array(pattern.into_iter().map(n).collect()),
                    n(phase),
                ],
            ),
            Op::new("J", vec![Object::Int(cap as i64)]),
        ];
        match self.kind {
            Kind::Line if double => {
                let length = self.u[0].hypot(self.u[1]).max(1e-9);
                let normal = [-self.u[1] / length, self.u[0] / length];
                for sign in [-1., 1.] {
                    let a = [
                        self.a[0] + sign * normal[0] * self.width / 3.,
                        self.a[1] + sign * normal[1] * self.width / 3.,
                    ];
                    out.push(Op::new("m", a.map(n).to_vec()));
                    out.push(Op::new("l", vec![n(a[0] + self.u[0]), n(a[1] + self.u[1])]));
                }
            }
            Kind::Line => out.extend([point("m", 0., 0.), point("l", 1., 0.)]),
            Kind::Circle => {
                let k = 0.2761423749153967;
                out.push(point("m", 1., 0.5));
                for v in [
                    [1., 0.5 + k, 0.5 + k, 1., 0.5, 1.],
                    [0.5 - k, 1., 0., 0.5 + k, 0., 0.5],
                    [0., 0.5 - k, 0.5 - k, 0., 0.5, 0.],
                    [0.5 + k, 0., 1., 0.5 - k, 1., 0.5],
                ] {
                    let mut args = Vec::new();
                    for pair in v.as_chunks::<2>().0 {
                        args.extend(self.at(pair[0], pair[1]).map(n));
                    }
                    out.push(Op::new("c", args));
                }
            }
            Kind::Rectangle | Kind::Table | Kind::FilledRectangle => {
                out.extend([
                    point("m", 0., 0.),
                    point("l", 1., 0.),
                    point("l", 1., 1.),
                    point("l", 0., 1.),
                    Op::new("h", vec![]),
                ]);
                if self.kind == Kind::Table {
                    for row in 1..self.rows {
                        let y = row as f64 / self.rows as f64;
                        out.extend([point("m", 0., y), point("l", 1., y)]);
                    }
                    for col in 1..self.columns {
                        let x = col as f64 / self.columns as f64;
                        out.extend([point("m", x, 0.), point("l", x, 1.)]);
                    }
                }
            }
        }
        out.extend([
            Op::new(
                if self.kind == Kind::FilledRectangle {
                    "f"
                } else {
                    "S"
                },
                vec![],
            ),
            Op::new("Q", vec![]),
        ]);
        out
    }
}
#[derive(Clone)]
struct State {
    matrix: Matrix,
    width: f64,
    color: [f64; 3],
    supported: bool,
    space: u8,
    dash: Vec<f64>,
    phase: f64,
    cap: u8,
    clip: Option<Clip>,
    fill: [f64; 3],
    fill_space: u8,
    fill_supported: bool,
}
#[derive(Clone, Copy)]
struct Clip([f64; 4]);
impl Clip {
    fn rectangle(segments: &[Drawing]) -> Option<Self> {
        let segments: Vec<_> = segments
            .iter()
            .filter(|s| s.u[0].hypot(s.u[1]) > 1e-6)
            .collect();
        if segments.len() != 4 {
            return None;
        }
        let mut r = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for (n, s) in segments.iter().enumerate() {
            let b = s.at(1., 0.);
            if (s.u[0].abs() > 1e-6 && s.u[1].abs() > 1e-6)
                || s.u[0].hypot(s.u[1]) < 1e-6
                || b.iter()
                    .zip(segments[(n + 1) % 4].a)
                    .any(|(a, b)| (a - b).abs() > 1e-6)
            {
                return None;
            }
            r[0] = r[0].min(s.a[0]);
            r[1] = r[1].min(s.a[1]);
            r[2] = r[2].max(s.a[0]);
            r[3] = r[3].max(s.a[1]);
        }
        let perimeter = segments.iter().map(|s| s.u[0].hypot(s.u[1])).sum::<f64>();
        ((perimeter - 2. * (r[2] - r[0] + r[3] - r[1])).abs() < 1e-5).then_some(Self(r))
    }
    fn intersect(self, other: Self) -> Self {
        Self([
            self.0[0].max(other.0[0]),
            self.0[1].max(other.0[1]),
            self.0[2].min(other.0[2]),
            self.0[3].min(other.0[3]),
        ])
    }
    fn contains(self, d: &Drawing) -> bool {
        // Only offer complete strokes; never expose the hidden part of a clipped line.
        let pad = if d.kind == Kind::FilledRectangle {
            0.
        } else {
            d.width / 2.
        };
        let points = if d.kind == Kind::Line {
            vec![d.a, d.at(1., 0.)]
        } else {
            vec![d.a, d.at(1., 0.), d.at(1., 1.), d.at(0., 1.)]
        };
        points.iter().all(|p| {
            p[0] - pad >= self.0[0] - 1e-5
                && p[1] - pad >= self.0[1] - 1e-5
                && p[0] + pad <= self.0[2] + 1e-5
                && p[1] + pad <= self.0[3] + 1e-5
        })
    }
}
struct Group {
    paint: usize,
    ops: Vec<usize>,
    segments: Vec<Drawing>,
    matrix: Matrix,
    clip: Option<Clip>,
}
struct Inventory {
    contents: Vec<Object>,
    pieces: Pieces,
    ops: Vec<Op>,
    groups: Vec<Group>,
    owned: Vec<Item>,
}
fn inventory(doc: &Document, page: usize) -> Result<Inventory> {
    inventory_mode(doc, page, false)
}
// Export consumes visible geometry, whereas editing needs complete, unclipped
// strokes so that changing a line cannot reveal previously hidden content.
fn inventory_mode(doc: &Document, page: usize, visible: bool) -> Result<Inventory> {
    let p = pdfcraft_model::pages(doc)
        .get(page)
        .cloned()
        .ok_or("Page introuvable.")?;
    let contents = p
        .dict
        .get(b"Contents")
        .map(|c| match doc.resolve(c).as_ref() {
            Object::Array(a) => a.clone(),
            _ => vec![c.clone()],
        })
        .unwrap_or_default();
    let mut chunks = Vec::new();
    let mut owned = Vec::new();
    let mut own_pieces = Vec::new();
    for (index, c) in contents.iter().enumerate() {
        let o = doc.resolve(c);
        let Object::Stream(s) = o.as_ref() else {
            return Err("Flux graphique non pris en charge.".into());
        };
        chunks.push(s.decoded().map_err(|e| e.to_string())?);
        if let Some(Object::String(data)) = s.dict.get(b"PNDrawing")
            && let Ok(drawing) = serde_json::from_str::<Drawing>(&data.to_text())
            && drawing.valid()
            && let Some(r) = c.as_ref()
        {
            owned.push(Item {
                target: Target::Owned(r),
                drawing,
            });
            own_pieces.push(index);
        }
    }
    let pieces = Pieces::join(&chunks);
    let parsed = pieces.parse();
    if parsed.skipped > 0 {
        return Err(
            "Ce flux PDF contient des tracés non interprétables. Aucun tracé n’est modifié.".into(),
        );
    }
    let ops = parsed.ops;
    let mut state = State {
        matrix: Matrix::IDENTITY,
        width: 1.,
        color: [0.; 3],
        supported: true,
        space: 1,
        dash: Vec::new(),
        phase: 0.,
        cap: 0,
        clip: None,
        fill: [0.; 3],
        fill_space: 1,
        fill_supported: true,
    };
    let mut stack = Vec::new();
    let mut groups = Vec::new();
    let mut path_ops = Vec::new();
    let mut segments = Vec::new();
    let mut cursor = None;
    let mut start = None;
    let mut simple = true;
    let mut in_text = false;
    let mut managed_text = Vec::new();
    let mut pending_clip: Option<Option<Clip>> = None;
    for (i, op) in ops.iter().enumerate() {
        let xy = |x, y| {
            let (x, y) = state.matrix.apply(x, y);
            [x, y]
        };
        match op.op.as_slice() {
            b"BDC" | b"BMC" => managed_text.push(
                op.operands
                    .get(1)
                    .and_then(Object::as_dict)
                    .is_some_and(|d| d.get(b"PNText").is_some()),
            ),
            b"EMC" => {
                managed_text.pop();
            }
            b"q" => {
                stack.push(state.clone());
                if !path_ops.is_empty() {
                    simple = false;
                }
            }
            b"Q" => {
                state = stack.pop().unwrap_or(State {
                    matrix: Matrix::IDENTITY,
                    width: 1.,
                    color: [0.; 3],
                    supported: false,
                    space: 1,
                    dash: Vec::new(),
                    phase: 0.,
                    cap: 0,
                    clip: None,
                    fill: [0.; 3],
                    fill_space: 1,
                    fill_supported: false,
                });
                if !path_ops.is_empty() {
                    simple = false;
                }
            }
            b"cm" => {
                if let Some(m) = op.nums::<6>() {
                    state.matrix = Matrix(m).then(&state.matrix);
                } else {
                    state.supported = false;
                }
                if !path_ops.is_empty() {
                    simple = false;
                }
            }
            b"w" => state.width = op.num(0).unwrap_or(1.),
            b"d" => {
                if let Some(pattern) = op
                    .operands
                    .first()
                    .and_then(Object::as_array)
                    .and_then(|a| a.iter().map(Object::as_f64).collect::<Option<Vec<_>>>())
                {
                    state.dash = pattern;
                    state.phase = op.num(1).unwrap_or(f64::NAN);
                    state.supported &=
                        StrokeStyle::from_pdf(&state.dash, state.phase, state.cap, 1.).valid();
                } else {
                    state.supported = false;
                }
            }
            b"J" => match op.num(0) {
                Some(v) if v == 0. || v == 1. || v == 2. => state.cap = v as u8,
                _ => state.supported = false,
            },
            b"RG" => {
                state.color = op.nums::<3>().unwrap_or([0.; 3]);
                state.space = 3;
            }
            b"G" => {
                state.color = [op.num(0).unwrap_or(0.); 3];
                state.space = 1;
            }
            b"K" => {
                state.space = 4;
                let c = op.nums::<4>().unwrap_or([0.; 4]);
                state.color = [
                    1. - (c[0] + c[3]).min(1.),
                    1. - (c[1] + c[3]).min(1.),
                    1. - (c[2] + c[3]).min(1.),
                ];
            }
            b"CS" => match op.name(0) {
                Some(b"DeviceRGB") => state.space = 3,
                Some(b"DeviceGray") => state.space = 1,
                Some(b"DeviceCMYK") => state.space = 4,
                _ => state.supported = false,
            },
            b"rg" => {
                state.fill = op.nums::<3>().unwrap_or([0.; 3]);
                state.fill_space = 3;
            }
            b"g" => {
                state.fill = [op.num(0).unwrap_or(0.); 3];
                state.fill_space = 1;
            }
            b"k" => {
                let c = op.nums::<4>().unwrap_or([0.; 4]);
                state.fill_space = 4;
                state.fill = [
                    1. - (c[0] + c[3]).min(1.),
                    1. - (c[1] + c[3]).min(1.),
                    1. - (c[2] + c[3]).min(1.),
                ];
            }
            b"cs" => {
                state.fill_space = match op.name(0) {
                    Some(b"DeviceRGB") => 3,
                    Some(b"DeviceGray") => 1,
                    Some(b"DeviceCMYK") => 4,
                    _ => {
                        state.fill_supported = false;
                        0
                    }
                };
            }
            b"sc" | b"scn" => match state.fill_space {
                1 => state.fill = [op.num(0).unwrap_or(0.); 3],
                3 => {
                    if let Some(c) = op.nums::<3>() {
                        state.fill = c;
                    } else {
                        state.fill_supported = false;
                    }
                }
                4 => {
                    if let Some(c) = op.nums::<4>() {
                        state.fill = [
                            1. - (c[0] + c[3]).min(1.),
                            1. - (c[1] + c[3]).min(1.),
                            1. - (c[2] + c[3]).min(1.),
                        ];
                    } else {
                        state.fill_supported = false;
                    }
                }
                _ => state.fill_supported = false,
            },
            b"SC" | b"SCN" => match state.space {
                1 => state.color = [op.num(0).unwrap_or(0.); 3],
                3 => {
                    if let Some(c) = op.nums::<3>() {
                        state.color = c;
                    } else {
                        state.supported = false;
                    }
                }
                4 => {
                    if let Some(c) = op.nums::<4>() {
                        state.color = [
                            1. - (c[0] + c[3]).min(1.),
                            1. - (c[1] + c[3]).min(1.),
                            1. - (c[2] + c[3]).min(1.),
                        ];
                    } else {
                        state.supported = false;
                    }
                }
                _ => state.supported = false,
            },
            b"gs" => {
                let gs = p
                    .dict
                    .get(b"Resources")
                    .and_then(|r| doc.dict(r))
                    .and_then(|r| r.get(b"ExtGState").and_then(|r| doc.dict(r)))
                    .and_then(|r| op.name(0).and_then(|k| r.get(k)).and_then(|r| doc.dict(r)));
                if let Some(gs) = gs {
                    if [b"CA".as_slice(), b"ca".as_slice()].iter().any(|k| {
                        gs.get(k)
                            .and_then(Object::as_f64)
                            .is_some_and(|v| (v - 1.).abs() > 1e-8)
                    }) {
                        state.fill_supported = false;
                    }
                    if let Some(w) = gs.get(b"LW").and_then(Object::as_f64) {
                        state.width = w;
                    }
                    if let Some(cap) = gs.get(b"LC").and_then(Object::as_int) {
                        if (0..=2).contains(&cap) {
                            state.cap = cap as u8;
                        } else {
                            state.supported = false;
                        }
                    }
                    if let Some(dash) = gs.get(b"D") {
                        if let Some(a) = dash.as_array() {
                            if let Some(pattern) =
                                a.first().and_then(Object::as_array).and_then(|a| {
                                    a.iter().map(Object::as_f64).collect::<Option<Vec<_>>>()
                                })
                            {
                                state.dash = pattern;
                                state.phase = a.get(1).and_then(Object::as_f64).unwrap_or(f64::NAN);
                                state.supported &=
                                    StrokeStyle::from_pdf(&state.dash, state.phase, state.cap, 1.)
                                        .valid();
                            } else {
                                state.supported = false;
                            }
                        } else {
                            state.supported = false;
                        }
                    }
                    if gs
                        .get(b"SMask")
                        .is_some_and(|v| v.as_name() != Some(b"None"))
                    {
                        state.supported = false;
                    }
                } else {
                    state.supported = false;
                }
            }
            b"W" | b"W*" => {
                pending_clip = Some(if simple {
                    Clip::rectangle(&segments)
                } else {
                    None
                })
            }
            b"BT" => in_text = true,
            b"Tr" if op.num(0).is_some_and(|v| v >= 4.) => state.supported = false,
            b"ET" => in_text = false,
            b"m" => {
                path_ops.push(i);
                if let Some(p) = op.nums::<2>() {
                    let p = xy(p[0], p[1]);
                    cursor = Some(p);
                    start = Some(p);
                } else {
                    simple = false;
                }
            }
            b"l" | b"h" => {
                path_ops.push(i);
                let end = if op.is("h") {
                    start
                } else {
                    op.nums::<2>().map(|p| xy(p[0], p[1]))
                };
                if let (Some(a), Some(b)) = (cursor, end) {
                    segments.push(Drawing {
                        kind: Kind::Line,
                        a,
                        u: [b[0] - a[0], b[1] - a[1]],
                        v: [0.; 2],
                        width: 1.,
                        color: [0.; 3],
                        style: StrokeStyle::Solid,
                        rows: 1,
                        columns: 1,
                    });
                    cursor = Some(b);
                } else {
                    simple = false;
                }
            }
            b"re" => {
                path_ops.push(i);
                if let Some([x, y, w, h]) = op.nums::<4>() {
                    let points = [xy(x, y), xy(x + w, y), xy(x + w, y + h), xy(x, y + h)];
                    for n in 0..4 {
                        let a = points[n];
                        let b = points[(n + 1) % 4];
                        segments.push(Drawing {
                            kind: Kind::Line,
                            a,
                            u: [b[0] - a[0], b[1] - a[1]],
                            v: [0.; 2],
                            width: 1.,
                            color: [0.; 3],
                            style: StrokeStyle::Solid,
                            rows: 1,
                            columns: 1,
                        });
                    }
                    cursor = Some(points[0]);
                    start = cursor;
                } else {
                    simple = false;
                }
            }
            b"c" | b"v" | b"y" => {
                path_ops.push(i);
                simple = false;
            }
            b"S" | b"s" | b"n" | b"f" | b"F" | b"f*" | b"B" | b"B*" | b"b" | b"b*" => {
                if op.is("s")
                    && let (Some(a), Some(b)) = (cursor, start)
                {
                    segments.push(Drawing {
                        kind: Kind::Line,
                        a,
                        u: [b[0] - a[0], b[1] - a[1]],
                        v: [0.; 2],
                        width: 1.,
                        color: [0.; 3],
                        style: StrokeStyle::Solid,
                        rows: 1,
                        columns: 1,
                    });
                }
                let m = state.matrix.0;
                let sx = m[0].hypot(m[1]);
                let sy = m[2].hypot(m[3]);
                let uniform = (sx - sy).abs() < 1e-5 * sx.max(1.)
                    && (m[0] * m[2] + m[1] * m[3]).abs() < 1e-5 * sx.max(1.);
                segments.retain(|s| s.u[0].hypot(s.u[1]) > 1e-6);
                if matches!(op.op.as_slice(), b"f" | b"F" | b"f*")
                    && simple
                    && state.supported
                    && state.fill_supported
                    && pending_clip.is_none()
                    && !in_text
                    && !managed_text.iter().any(|v| *v)
                    && !own_pieces.contains(&pieces.piece_of(op.span.start))
                    && let Some(drawing) = filled_shape(&segments, state.fill)
                    && state.clip.is_none_or(|c| c.contains(&drawing))
                {
                    groups.push(Group {
                        paint: i,
                        ops: path_ops.clone(),
                        segments: vec![drawing],
                        matrix: state.matrix,
                        clip: state.clip,
                    });
                }
                if (op.is("S") || op.is("s"))
                    && simple
                    && state.supported
                    && uniform
                    && !in_text
                    && !managed_text.iter().any(|v| *v)
                    && !segments.is_empty()
                    && !own_pieces.contains(&pieces.piece_of(op.span.start))
                    && pending_clip.is_none()
                {
                    for segment in &mut segments {
                        segment.width = (state.width * sx).max(0.05);
                        segment.color = state.color;
                        segment.style =
                            StrokeStyle::from_pdf(&state.dash, state.phase, state.cap, sx);
                    }
                    if visible && let Some(clip) = state.clip {
                        segments.retain_mut(|s| {
                            let b = s.at(1., 0.);
                            let r = clip.0;
                            if s.u[1].abs() < 1e-6 && s.a[1] >= r[1] && s.a[1] <= r[3] {
                                let a = s.a[0].min(b[0]).max(r[0]);
                                let z = s.a[0].max(b[0]).min(r[2]);
                                s.a[0] = a;
                                s.u[0] = z - a;
                                z > a
                            } else if s.u[0].abs() < 1e-6 && s.a[0] >= r[0] && s.a[0] <= r[2] {
                                let a = s.a[1].min(b[1]).max(r[1]);
                                let z = s.a[1].max(b[1]).min(r[3]);
                                s.a[1] = a;
                                s.u[1] = z - a;
                                z > a
                            } else {
                                clip.contains(s)
                            }
                        });
                    }
                    if visible
                        || state
                            .clip
                            .is_none_or(|clip| segments.iter().all(|s| clip.contains(s)))
                    {
                        groups.push(Group {
                            paint: i,
                            ops: path_ops.clone(),
                            segments: segments.clone(),
                            matrix: state.matrix,
                            clip: state.clip,
                        });
                    }
                }
                if let Some(clip) = pending_clip.take() {
                    if let Some(clip) = clip {
                        state.clip = Some(state.clip.map_or(clip, |old| old.intersect(clip)));
                    } else {
                        state.supported = false;
                    }
                }
                path_ops.clear();
                segments.clear();
                cursor = None;
                start = None;
                simple = true;
            }
            _ => {}
        }
    }
    Ok(Inventory {
        contents,
        pieces,
        ops,
        groups,
        owned,
    })
}
fn filled_shape(segments: &[Drawing], color: [f64; 3]) -> Option<Drawing> {
    if !(3..=12).contains(&segments.len())
        || segments.iter().enumerate().any(|(n, s)| {
            s.at(1., 0.)
                .iter()
                .zip(segments[(n + 1) % segments.len()].a)
                .any(|(a, b)| (a - b).abs() > 1e-5)
        })
    {
        return None;
    }
    let r = segments.iter().fold(
        [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ],
        |r, s| {
            [
                r[0].min(s.a[0]),
                r[1].min(s.a[1]),
                r[2].max(s.a[0]),
                r[3].max(s.a[1]),
            ]
        },
    );
    let (w, h) = (r[2] - r[0], r[3] - r[1]);
    if w <= 0. || h <= 0. {
        return None;
    }
    let area = segments
        .iter()
        .map(|s| {
            let b = s.at(1., 0.);
            s.a[0] * b[1] - s.a[1] * b[0]
        })
        .sum::<f64>()
        .abs()
        / 2.;
    let thin = (h <= 3. && w >= h * 8.) || (w <= 3. && h >= w * 8.);
    let mut sign = 0.;
    for (n, s) in segments.iter().enumerate() {
        let next = &segments[(n + 1) % segments.len()];
        let cross = s.u[0] * next.u[1] - s.u[1] * next.u[0];
        // Writer rounds bevel coordinates to tenths of a point. Tiny opposite
        // turns at those joins are rounding, not a genuinely concave shape.
        if cross.abs() > (w.min(h).powi(2) * 0.3).max(1e-7) {
            if sign != 0. && cross.signum() != sign {
                return None;
            }
            sign = cross.signum();
        }
    }
    let (kind, a, u, v, width) = if thin && area >= w * h * 0.65 && area <= w * h * 1.00001 {
        if w >= h {
            (
                Kind::Line,
                [r[0], (r[1] + r[3]) / 2.],
                [w, 0.],
                [0.; 2],
                h.max(0.05),
            )
        } else {
            (
                Kind::Line,
                [(r[0] + r[2]) / 2., r[1]],
                [0., h],
                [0.; 2],
                w.max(0.05),
            )
        }
    } else if let Some((a, u, thickness)) = oblique_stroke(segments, area) {
        (Kind::Line, a, u, [0.; 2], thickness)
    } else if Clip::rectangle(segments).is_some() {
        (Kind::FilledRectangle, [r[0], r[1]], [w, 0.], [0., h], 1.)
    } else {
        return None;
    };
    Some(Drawing {
        kind,
        a,
        u,
        v,
        width,
        color,
        style: StrokeStyle::Solid,
        rows: 1,
        columns: 1,
    })
}
/// Writer also paints diagonal strokes as narrow, bevelled filled polygons.
/// Measure along their own axis instead of their large axis-aligned bounds.
fn oblique_stroke(segments: &[Drawing], area: f64) -> Option<([f64; 2], [f64; 2], f64)> {
    let edge = segments
        .iter()
        .max_by(|a, b| a.u[0].hypot(a.u[1]).total_cmp(&b.u[0].hypot(b.u[1])))?;
    let length = edge.u[0].hypot(edge.u[1]);
    if length < 1e-6 || edge.u[0].abs() < 1e-6 || edge.u[1].abs() < 1e-6 {
        return None;
    }
    let sign = if edge.u[0] < 0. { -1. } else { 1. };
    let axis = edge.u.map(|v| v / length * sign);
    let normal = [-axis[1], axis[0]];
    let bounds = segments.iter().fold(
        [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ],
        |r, s| {
            let along = s.a[0] * axis[0] + s.a[1] * axis[1];
            let across = s.a[0] * normal[0] + s.a[1] * normal[1];
            [
                r[0].min(along),
                r[1].min(across),
                r[2].max(along),
                r[3].max(across),
            ]
        },
    );
    let span = bounds[2] - bounds[0];
    let thickness = bounds[3] - bounds[1];
    if thickness <= 0.
        || thickness > 3.
        || span < thickness * 8.
        || area < span * thickness * 0.65
        || area > span * thickness * 1.00001
    {
        return None;
    }
    let across = (bounds[1] + bounds[3]) * 0.5;
    Some((
        [
            axis[0] * bounds[0] + normal[0] * across,
            axis[1] * bounds[0] + normal[1] * across,
        ],
        axis.map(|v| v * span),
        thickness.max(0.05),
    ))
}
fn stream(drawing: &Drawing) -> Result<Stream> {
    if !drawing.valid() {
        return Err("Les dimensions ou le style du dessin sont invalides.".into());
    }
    let mut dict = Dict::new();
    dict.set(
        b"PNDrawing".to_vec(),
        Object::String(PdfString::text(
            &serde_json::to_string(drawing).map_err(|e| e.to_string())?,
        )),
    );
    Ok(Stream::flate(
        dict,
        &pdfcraft_content::serialize_ops(&drawing.ops()),
    ))
}
fn append_drawing(doc: &mut Document, page: usize, drawing: &Drawing) -> Result<ObjRef> {
    let stream = stream(drawing)?;
    let data = stream.decoded().map_err(|e| e.to_string())?;
    pdfcraft_edit::stamp(doc, page, "PDFinatorDrawing", data).map_err(|e| e.to_string())?;
    let p = pdfcraft_model::pages(doc)
        .get(page)
        .cloned()
        .ok_or("Page introuvable.")?;
    let contents = p
        .dict
        .get(b"Contents")
        .and_then(|o| doc.resolve(o).as_array().cloned())
        .ok_or("Flux de dessin introuvable.")?;
    let created = contents
        .last()
        .and_then(Object::as_ref)
        .ok_or("Dessin introuvable.")?;
    doc.update_dict(created, |d| {
        if let Some(metadata) = stream.dict.get(b"PNDrawing") {
            d.set(b"PNDrawing".to_vec(), metadata.clone());
        }
    })
    .map_err(|e| e.to_string())?;
    Ok(created)
}
fn apply(doc: &mut Document, page: usize, changes: &[(Target, Option<Drawing>)]) -> Result<()> {
    let all = inventory(doc, page)?;
    let mut edits = Vec::new();
    let mut contents = all.contents.clone();
    let mut detached = Vec::new();
    for (target, drawing) in changes {
        if let Target::Path(p, s) = target
            && !all
                .groups
                .iter()
                .any(|g| g.paint == *p && *s < g.segments.len())
        {
            return Err("Le trait sélectionné n’existe plus.".into());
        }
        if let Some(drawing) = drawing
            && !drawing.valid()
        {
            return Err("Dessin invalide.".into());
        }
        if let Target::Owned(r) = target {
            if !all.owned.iter().any(|i| i.target == *target) {
                return Err("Le dessin sélectionné n’existe plus.".into());
            }
            if let Some(drawing) = drawing {
                doc.set(*r, Object::Stream(stream(drawing)?));
            } else {
                contents.retain(|o| o.as_ref() != Some(*r));
            }
        }
    }
    for group in &all.groups {
        if !changes
            .iter()
            .any(|(t, _)| matches!(t,Target::Path(p,_) if *p==group.paint))
        {
            continue;
        }
        let inverse = group
            .matrix
            .invert()
            .ok_or("Ce tracé a une transformation non inversible.")?;
        let desired = group
            .segments
            .iter()
            .enumerate()
            .map(|(i, original)| {
                changes
                    .iter()
                    .find(|(t, _)| *t == Target::Path(group.paint, i))
                    .map_or(Some(original), |(_, d)| {
                        if let Some(drawing) = d.as_ref()
                            && group.clip.is_some_and(|c| !c.contains(drawing))
                        {
                            detached.push(drawing.clone());
                            None
                        } else {
                            d.as_ref()
                        }
                    })
            })
            .collect::<Vec<_>>();
        let translation = desired
            .first()
            .copied()
            .flatten()
            .map(|d| {
                [
                    d.a[0] - group.segments[0].a[0],
                    d.a[1] - group.segments[0].a[1],
                ]
            })
            .filter(|delta| {
                group.segments.iter().zip(&desired).all(|(old, new)| {
                    let mut moved = old.clone();
                    moved.translate(*delta);
                    new.is_some_and(|new| moved == *new)
                })
            });
        let mut replacements = if let Some(delta) = translation {
            // Translate the original operators. Keep Writer's bevelled borders,
            // fill rule, joins and caps exactly when no styling was changed.
            let a = inverse.apply(0., 0.);
            let b = inverse.apply(delta[0], delta[1]);
            let mut ops = vec![
                Op::new("q", vec![]),
                Op::new(
                    "cm",
                    [1., 0., 0., 1., b.0 - a.0, b.1 - a.1]
                        .map(pdfcraft_content::num)
                        .to_vec(),
                ),
            ];
            ops.extend(group.ops.iter().map(|i| all.ops[*i].clone()));
            ops.push(all.ops[group.paint].clone());
            ops
        } else {
            vec![
                Op::new("q", vec![]),
                Op::new("cm", inverse.0.map(pdfcraft_content::num).to_vec()),
            ]
        };
        if translation.is_none() {
            for drawing in desired.into_iter().flatten() {
                replacements.extend(drawing.ops());
            }
        }
        replacements.push(Op::new("Q", vec![]));
        for i in &group.ops {
            edits.push((all.ops[*i].span.clone(), Vec::new()));
        }
        edits.push((
            all.ops[group.paint].span.clone(),
            pdfcraft_content::serialize_ops(&replacements),
        ));
    }
    edits.sort_by_key(|e| e.0.start);
    let chunks = all.pieces.splice(edits);
    for (i, (old, data)) in all.contents.iter().zip(chunks).enumerate() {
        if changes
            .iter()
            .any(|(target, _)| matches!(target,Target::Owned(r) if old.as_ref()==Some(*r)))
        {
            continue;
        }
        let old_obj = doc.resolve(old);
        if let Object::Stream(s) = old_obj.as_ref()
            && s.decoded().map_err(|e| e.to_string())? != data
        {
            let mut dict = s.dict.clone();
            dict.remove(b"Filter");
            dict.remove(b"DecodeParms");
            let new = doc.add(Object::Stream(Stream::flate(dict, &data)));
            if let Some(at) = contents.iter().position(|o| o == old) {
                contents[at] = Object::Ref(new);
            } else if i >= contents.len() {
                continue;
            }
        }
    }
    let p = pdfcraft_model::pages(doc)
        .get(page)
        .cloned()
        .ok_or("Page introuvable.")?;
    doc.update_dict(p.obj, |d| {
        d.set(b"Contents".to_vec(), Object::Array(contents))
    })
    .map_err(|e| e.to_string())?;
    for drawing in detached {
        append_drawing(doc, page, &drawing)?;
    }
    Ok(())
}
impl Editor {
    pub fn delete_drawing_table(
        &mut self,
        page: usize,
        changes: &[(Target, Option<Drawing>)],
        bounds: [f64; 4],
    ) -> Result<()> {
        let mut targets = pdfcraft_edit::editable_objects(&self.doc, page)
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|o| {
                o.rect[0] >= bounds[0] - 1.
                    && o.rect[1] >= bounds[1] - 1.
                    && o.rect[2] <= bounds[2] + 1.
                    && o.rect[3] <= bounds[3] + 1.
            })
            .map(|o| o.target)
            .collect::<Vec<_>>();
        targets.sort_by_key(|a| std::cmp::Reverse(a.index));
        self.change(false, |doc| {
            apply(doc, page, changes)?;
            for kind in [
                pdfcraft_edit::ObjectKind::Text,
                pdfcraft_edit::ObjectKind::Image,
                pdfcraft_edit::ObjectKind::Added,
            ] {
                for t in targets.iter().filter(|t| t.kind == kind) {
                    match kind {
                        pdfcraft_edit::ObjectKind::Text => {
                            pdfcraft_edit::clear_block(doc, page, t.index)
                        }
                        pdfcraft_edit::ObjectKind::Image => pdfcraft_edit::change_image(
                            doc,
                            page,
                            t.index,
                            &pdfcraft_edit::ImageChange::Delete,
                        ),
                        pdfcraft_edit::ObjectKind::Added => {
                            pdfcraft_edit::delete_content(doc, page, t.index)
                        }
                    }
                    .map_err(|e| e.to_string())?;
                }
            }
            Ok(())
        })
    }
    /// Move a table's vector elements and contained native PDF objects as one transaction.
    pub fn move_drawing_table(
        &mut self,
        page: usize,
        changes: &[(Target, Option<Drawing>)],
        bounds: [f64; 4],
        delta: [f64; 2],
    ) -> Result<()> {
        let targets = pdfcraft_edit::editable_objects(&self.doc, page)
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|o| {
                o.rect[0] >= bounds[0] - 1.
                    && o.rect[1] >= bounds[1] - 1.
                    && o.rect[2] <= bounds[2] + 1.
                    && o.rect[3] <= bounds[3] + 1.
            })
            .map(|o| o.target)
            .collect::<Vec<_>>();
        if targets.iter().any(|t| {
            t.kind == pdfcraft_edit::ObjectKind::Text
                && self.text_formatting(page, t.index).is_some()
        }) {
            return Err("Ce tableau contient du texte déjà remis en forme. Déplacez ses éléments séparément pour conserver cette mise en forme.".into());
        }
        self.change(false, |doc| {
            apply(doc, page, changes)?;
            if !targets.is_empty() {
                pdfcraft_edit::objects::move_objects(doc, page, &targets, delta)
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        })
    }
    pub fn drawings(&self, page: usize) -> Result<Vec<Item>> {
        guarded(|| {
            let all = inventory(&self.doc, page)?;
            let mut out = all.owned;
            for group in all.groups {
                for (i, drawing) in group.segments.into_iter().enumerate() {
                    out.push(Item {
                        target: Target::Path(group.paint, i),
                        drawing,
                    });
                }
            }
            Ok(out)
        })
    }
    pub(crate) fn export_drawings(&self, page: usize) -> Result<Vec<Drawing>> {
        let all = inventory_mode(&self.doc, page, true)?;
        Ok(all
            .owned
            .into_iter()
            .map(|i| i.drawing)
            .chain(all.groups.into_iter().flat_map(|g| g.segments))
            .collect())
    }
    pub fn add_drawing(&mut self, page: usize, drawing: Drawing) -> Result<ObjRef> {
        let mut created = ObjRef::new(0, 0);
        self.change(false, |doc| {
            created = append_drawing(doc, page, &drawing)?;
            Ok(())
        })?;
        Ok(created)
    }
    pub fn change_drawings(
        &mut self,
        page: usize,
        changes: &[(Target, Option<Drawing>)],
    ) -> Result<()> {
        if changes.is_empty() {
            return Ok(());
        }
        self.change(false, |doc| apply(doc, page, changes))
    }
    pub fn split_drawing(&mut self, page: usize, target: Target) -> Result<Vec<Target>> {
        let drawing = self
            .drawings(page)?
            .into_iter()
            .find(|i| i.target == target)
            .ok_or("Dessin introuvable.")?
            .drawing;
        if !matches!(drawing.kind, Kind::Rectangle | Kind::Table) {
            return Err("Ce dessin n’est pas un rectangle ou un tableau.".into());
        }
        let mut segments = Vec::new();
        let mut add = |x0, y0, x1, y1| {
            let a = drawing.at(x0, y0);
            let b = drawing.at(x1, y1);
            segments.push(Drawing {
                kind: Kind::Line,
                a,
                u: [b[0] - a[0], b[1] - a[1]],
                v: [0.; 2],
                width: drawing.width,
                color: drawing.color,
                style: drawing.style.clone(),
                rows: 1,
                columns: 1,
            });
        };
        add(0., 0., 1., 0.);
        add(1., 0., 1., 1.);
        add(1., 1., 0., 1.);
        add(0., 1., 0., 0.);
        if drawing.kind == Kind::Table {
            for r in 1..drawing.rows {
                let y = r as f64 / drawing.rows as f64;
                add(0., y, 1., y);
            }
            for c in 1..drawing.columns {
                let x = c as f64 / drawing.columns as f64;
                add(x, 0., x, 1.);
            }
        }
        let mut targets = Vec::new();
        self.change(false, |doc| {
            apply(doc, page, &[(target, None)])?;
            for segment in segments {
                let r = append_drawing(doc, page, &segment)?;
                targets.push(Target::Owned(r));
            }
            Ok(())
        })?;
        Ok(targets)
    }
    pub fn preview_without_drawings(
        &self,
        page: usize,
        targets: &[Target],
    ) -> Result<std::sync::Arc<Vec<u8>>> {
        guarded(|| {
            let mut doc = self.doc.clone();
            apply(
                &mut doc,
                page,
                &targets.iter().map(|t| (*t, None)).collect::<Vec<_>>(),
            )?;
            pdfcraft_cos::write_incremental(&doc, &SaveOptions::default())
                .map(std::sync::Arc::new)
                .map_err(|e| e.to_string())
        })
    }
}
