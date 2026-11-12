/*
* A picture of the prime-counting automaton (`prime_display`) at work.
*
* Each cell of the triangle stands for one number. On its way to being counted a cell shows, in this
* order: testing divisors; knows whether it is prime; knows the step at which it turns set; set
* ("lit"). Cells of the table rows (1..=BOOTSTRAP_ROWS) know their step as soon as they are created,
* so they skip the first two. `record` runs the automaton and notes when each cell of the first rows
* changed its look; `play` animates that in the terminal (the `demo` command) and `svg` draws the
* animation shown in the README.
*/

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Write as _;
use std::time::Duration;

use crate::prime_display::{Cell, Simulation, BOOTSTRAP_ROWS};

/// What a cell shows, declared in the order in which a cell goes through them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Look {
    Absent,
    Testing,
    Composite,
    Prime,
    Ready,
    Lit,
}

impl Look {
    fn label(self) -> &'static str {
        match self {
            Look::Absent => "not built yet",
            Look::Testing => "testing divisors",
            Look::Composite => "found it is not prime",
            Look::Prime => "found it is prime",
            Look::Ready => "knows when to light up",
            Look::Lit => "lit (counted)",
        }
    }

    fn hex(self) -> &'static str {
        match self {
            Look::Absent => "#0d1117",
            Look::Testing => "#3d444d",
            Look::Composite => "#768390",
            Look::Prime => "#f0883e",
            Look::Ready => "#2f81f7",
            Look::Lit => "#f2cc60",
        }
    }

    /// xterm 256-colour code.
    fn ansi(self) -> u8 {
        match self {
            Look::Absent => 0,
            Look::Testing => 238,
            Look::Composite => 245,
            Look::Prime => 208,
            Look::Ready => 33,
            Look::Lit => 220,
        }
    }

    fn glyph(self) -> char {
        match self {
            Look::Absent => ' ',
            Look::Testing => ':',
            Look::Composite => '.',
            Look::Prime => '*',
            Look::Ready => '+',
            Look::Lit => '#',
        }
    }
}

/// Colour of the cells that turned set in the current step.
const FLASH_HEX: &str = "#ffffff";
const FLASH_ANSI: u8 = 231;

/// The steps at which one cell changed its look.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct History {
    pub created: u64,
    /// When it learned whether it is prime, and the answer (table cells never test).
    pub verdict: Option<(u64, bool)>,
    pub ready: Option<u64>,
    pub lit: Option<u64>,
}

impl History {
    pub fn look(&self, t: u64) -> Look {
        let reached = |at: Option<u64>| at.is_some_and(|at| at <= t);
        if t < self.created {
            Look::Absent
        } else if reached(self.lit) {
            Look::Lit
        } else if reached(self.ready) {
            Look::Ready
        } else {
            match self.verdict {
                Some((at, true)) if at <= t => Look::Prime,
                Some((at, false)) if at <= t => Look::Composite,
                _ => Look::Testing,
            }
        }
    }
}

/// First number of a row.
pub fn row_start(row: u64) -> u64 {
    row * (row - 1) / 2 + 1
}

pub struct Recording {
    pub rows: u64,
    /// The history of number n at index n - 1, for the numbers of rows 1..=rows.
    pub cells: Vec<History>,
    /// (x, row) of number n at index n - 1.
    positions: Vec<(u64, u64)>,
    /// counts[t]: set cells after step t, the t-th prime (counts[0] = 1 is the seed).
    pub counts: Vec<u64>,
}

/// Runs the automaton from its seed for as long as every set cell lies in rows 1..=rows, and notes
/// when each cell of those rows changed its look.
pub fn record(rows: u64) -> Recording {
    let last = row_start(rows + 1) - 1;
    let mut cells: Vec<Option<History>> = vec![None; last as usize];
    let mut counts = vec![];
    let mut sim = Simulation::new();
    while sim.set_count() <= last {
        let t = sim.time;
        counts.push(sim.set_count());
        for (_, y, cell) in sim.cells() {
            if y as u64 > rows {
                continue;
            }
            if let Cell::Tile(tile) = cell {
                let h = cells[(tile.n - 1) as usize].get_or_insert(History {
                    created: t,
                    ..History::default()
                });
                if h.verdict.is_none() {
                    h.verdict = tile.prime.map(|prime| (t, prime));
                }
                if h.ready.is_none() && tile.tau.is_some() {
                    h.ready = Some(t);
                }
                if h.lit.is_none() && tile.set {
                    h.lit = Some(t);
                }
            }
        }
        sim.step();
    }
    let positions = (1..=rows)
        .flat_map(|row| (0..row).map(move |x| (x, row)))
        .collect();
    Recording {
        rows,
        cells: cells
            .into_iter()
            .map(|h| h.expect("every cell of the rows exists"))
            .collect(),
        positions,
        counts,
    }
}

impl Recording {
    /// The last recorded step.
    pub fn steps(&self) -> u64 {
        self.counts.len() as u64 - 1
    }

    pub fn look(&self, x: u64, row: u64, t: u64) -> Look {
        if row == 0 || row > self.rows || x >= row {
            return Look::Absent;
        }
        self.cells[(row_start(row) + x - 1) as usize].look(t)
    }

    /// The numbers that turned set at step t.
    fn just_lit(&self, t: u64) -> std::ops::RangeInclusive<u64> {
        if t == 0 {
            return 1..=0;
        }
        self.counts[t as usize - 1] + 1..=self.counts[t as usize]
    }
}

fn ordinal(k: u64) -> String {
    let suffix = match (k % 10, k % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{}{}", k, suffix)
}

pub fn caption(rec: &Recording, t: u64) -> String {
    let p = rec.counts[t as usize];
    if t == 0 {
        "step 0: 1 cell is lit (the starting cell)".to_string()
    } else {
        format!(
            "step {}: {} cells are lit, and {} is the {} prime",
            t,
            p,
            p,
            ordinal(t)
        )
    }
}

/// Rows 1..=rows, top row first, with the number of every cell; lit cells are highlighted (white
/// for those lit in this step) or, without colour, put in brackets.
pub fn numbers_frame(rec: &Recording, rows: u64, t: u64, colour: bool) -> String {
    let fresh = rec.just_lit(t);
    let mut out = String::new();
    for row in (1..=rows).rev() {
        out.push_str("  ");
        for x in 0..row {
            let n = row_start(row) + x;
            let look = rec.look(x, row, t);
            let _ = match (look, colour) {
                (Look::Absent, true) => write!(out, "    "),
                (Look::Absent, false) => write!(out, "     "),
                (Look::Lit, true) => {
                    let bg = if fresh.contains(&n) {
                        FLASH_ANSI
                    } else {
                        Look::Lit.ansi()
                    };
                    write!(out, "\x1b[48;5;{}m\x1b[38;5;16m{:>3}\x1b[0m ", bg, n)
                }
                (_, true) => write!(out, "\x1b[38;5;240m{:>3}\x1b[0m ", n),
                (Look::Lit, false) => write!(out, "[{:>3}]", n),
                (_, false) => write!(out, " {:>3} ", n),
            };
        }
        out.push('\n');
    }
    out
}

/// Rows first_row..=rec.rows in colour, two rows per line of text (the upper half block shows the
/// upper row).
pub fn picture_frame(rec: &Recording, first_row: u64, t: u64) -> String {
    let fresh = rec.just_lit(t);
    let colour = |x: u64, row: u64| -> Option<u8> {
        if row < first_row {
            return None;
        }
        match rec.look(x, row, t) {
            Look::Absent => None,
            Look::Lit if fresh.contains(&(row_start(row) + x)) => Some(FLASH_ANSI),
            look => Some(look.ansi()),
        }
    };
    let mut out = String::new();
    let mut upper = rec.rows;
    loop {
        out.push_str("  ");
        let mut current = (None, None);
        for x in 0..rec.rows {
            let lower = if upper > 1 {
                colour(x, upper - 1)
            } else {
                None
            };
            let (glyph, fg, bg) = match (colour(x, upper), lower) {
                (None, None) => (' ', None, None),
                (Some(a), None) => ('▀', Some(a), None),
                (None, Some(b)) => ('▄', Some(b), None),
                (Some(a), Some(b)) => ('▀', Some(a), Some(b)),
            };
            if (fg, bg) != current {
                out.push_str("\x1b[0m");
                if let Some(f) = fg {
                    let _ = write!(out, "\x1b[38;5;{}m", f);
                }
                if let Some(b) = bg {
                    let _ = write!(out, "\x1b[48;5;{}m", b);
                }
                current = (fg, bg);
            }
            out.push(glyph);
        }
        out.push_str("\x1b[0m\n");
        if upper <= first_row + 1 {
            break;
        }
        upper -= 2;
    }
    out
}

/// Every row, one character per cell (see `Look::glyph`).
pub fn plain_picture(rec: &Recording, t: u64) -> String {
    let mut out = String::new();
    for row in (1..=rec.rows).rev() {
        out.push_str("  ");
        out.extend((0..row).map(|x| rec.look(x, row, t).glyph()));
        out.push('\n');
    }
    out
}

fn legend(colour: bool) -> String {
    let items = [
        (Look::Lit.ansi(), '#', "lit"),
        (FLASH_ANSI, '#', "just lit"),
        (Look::Ready.ansi(), '+', "knows when to light up"),
        (Look::Prime.ansi(), '*', "prime"),
        (Look::Composite.ansi(), '.', "not prime"),
        (Look::Testing.ansi(), ':', "testing divisors"),
    ];
    let mut out = String::new();
    for line in items.chunks(3) {
        out.push_str("  ");
        for &(code, glyph, label) in line {
            if colour {
                let _ = write!(out, "\x1b[38;5;{}m██\x1b[0m {}   ", code, label);
            } else if label != "just lit" {
                let _ = write!(out, "{} {}   ", glyph, label);
            }
        }
        out.push('\n');
    }
    out
}

/// Lines of the terminal, if `stty` can tell.
pub fn terminal_lines() -> Option<u64> {
    let tty = std::fs::File::open("/dev/tty").ok()?;
    let output = std::process::Command::new("stty")
        .arg("size")
        .stdin(tty)
        .output()
        .ok()?;
    String::from_utf8(output.stdout)
        .ok()?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

pub struct PlayOptions {
    /// Frames per second relative to the default.
    pub speed: f64,
    /// No colours and no animation: print a selection of frames one after the other.
    pub plain: bool,
    /// Lines of the terminal.
    pub lines: u64,
}

/// Rows shown up close, with their numbers.
const CLOSE_ROWS: u64 = 16;

/// The terminal demo: the first rows up close with their numbers, then every row of the recording
/// with what its cells are doing.
pub fn play(rec: &Recording, options: &PlayOptions) -> std::io::Result<()> {
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    let pause = |ms: f64| {
        if !options.plain {
            std::thread::sleep(Duration::from_secs_f64(ms / 1000.0 / options.speed));
        }
    };
    let mut show = |frame: &str| -> std::io::Result<()> {
        if options.plain {
            writeln!(out, "{}", frame)?;
        } else {
            // Redraw in place: home, every line cleared to its end, then the rest of the screen.
            write!(out, "\x1b[H{}\x1b[J", frame.replace('\n', "\x1b[K\n"))?;
        }
        out.flush()
    };
    if !options.plain {
        show("\x1b[2J")?;
    }
    let close_rows = CLOSE_ROWS.min(rec.rows);
    let close_last = row_start(close_rows + 1) - 1;
    let close_steps = rec.counts.iter().take_while(|&&c| c <= close_last).count() as u64;
    for t in 0..close_steps {
        let mut frame = format!(
            "The first {} rows up close: one cell per number, 1 at the bottom.\n\
             After each step the lit cells are 1, 2, 3, ... up to the next prime.\n\n",
            close_rows
        );
        frame += &numbers_frame(rec, close_rows, t, !options.plain);
        frame += &format!("\n  {}\n", caption(rec, t));
        show(&frame)?;
        pause(if t < 8 { 1000.0 } else { 500.0 });
    }
    if close_steps > rec.steps() {
        return Ok(());
    }
    pause(1500.0);
    let available = options.lines.saturating_sub(11).max(4);
    let shown = rec.rows.min(2 * available);
    let first_row = rec.rows - shown + 1;
    for t in close_steps..=rec.steps() {
        if options.plain && t % 40 != 0 && t != rec.steps() {
            continue;
        }
        let mut frame = format!(
            "Zoomed out: rows {}-{}{}.\n\
             Rows 1-{} know when to light up from a table in the rule;\n\
             from row {} on, the cells work it out for themselves.\n\n",
            first_row,
            rec.rows,
            if options.plain {
                ""
            } else {
                ", two rows per line of text"
            },
            BOOTSTRAP_ROWS,
            BOOTSTRAP_ROWS + 1
        );
        frame += &if options.plain {
            plain_picture(rec, t)
        } else {
            picture_frame(rec, first_row, t)
        };
        frame += "\n";
        frame += &legend(!options.plain);
        frame += &format!("\n  {}\n", caption(rec, t));
        show(&frame)?;
        pause(if t < 50 { 150.0 } else { 60.0 });
    }
    let next = rec.counts[rec.steps() as usize];
    writeln!(
        out,
        "\n  The next prime after {} lies beyond row {}, but the automaton keeps going:\n  \
         `cargo run --release -- primeca --steps 20000` checks the first 20,000 steps.",
        next, rec.rows
    )?;
    out.flush()
}

/// Seconds that step t stays on screen in the SVG animation.
fn frame_seconds(t: u64) -> f64 {
    if t < 10 {
        0.7
    } else if t < 40 {
        0.15
    } else {
        0.06
    }
}

/// The last frame stays this long before the animation starts again.
const HOLD_SECONDS: f64 = 3.0;
/// Newly lit cells stay white this long (at most one frame).
const FLASH_SECONDS: f64 = 0.5;

/// A discrete SMIL animation: the value of an attribute from each time on (changes only).
struct Track {
    times: Vec<f64>,
    values: Vec<String>,
}

impl Track {
    fn new() -> Self {
        Track {
            times: vec![],
            values: vec![],
        }
    }

    fn set(&mut self, time: f64, value: impl Into<String>) {
        let value = value.into();
        if self.values.last() != Some(&value) {
            self.times.push(time);
            self.values.push(value);
        }
    }

    /// The value at the end, used as the static value (shown where animation is not supported).
    fn last(&self) -> &str {
        self.values.last().map_or("0", |v| v.as_str())
    }

    fn animate(&self, element: &str, attribute: &str, total: f64) -> String {
        if self.values.len() < 2 {
            return String::new();
        }
        let times: Vec<String> = self
            .times
            .iter()
            .map(|t| format!("{:.5}", t / total))
            .collect();
        let kind = if element == "animateTransform" {
            " type=\"translate\""
        } else {
            ""
        };
        format!(
            "<{} attributeName=\"{}\"{} values=\"{}\" keyTimes=\"{}\" calcMode=\"discrete\" dur=\"{:.2}s\" repeatCount=\"indefinite\"/>",
            element,
            attribute,
            kind,
            self.values.join(";"),
            times.join(";"),
            total
        )
    }
}

/// A right-aligned number whose digits change over time: each digit is a vertical strip of the
/// glyphs blank, 0, ..., 9 behind a one-digit window, moved by one animation.
#[allow(clippy::too_many_arguments)]
fn digit_field(
    defs: &mut String,
    body: &mut String,
    id: &str,
    right: f64,
    baseline: f64,
    font: f64,
    class: &str,
    slots: u32,
    values: &[(f64, u64)],
    total: f64,
) {
    let advance = font * 0.62;
    let gap = font * 1.5;
    for slot in 0..slots {
        let x = right - slot as f64 * advance;
        let unit = 10u64.pow(slot);
        let mut track = Track::new();
        for &(time, value) in values {
            let glyph = if slot == 0 || value >= unit {
                (value / unit) % 10 + 1
            } else {
                0
            };
            track.set(time, format!("0 {}", -(glyph as f64) * gap));
        }
        let _ = writeln!(
            defs,
            "<clipPath id=\"{id}{slot}\"><rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\"/></clipPath>",
            x - advance,
            baseline - font * 0.85,
            advance,
            font * 1.15
        );
        let _ = write!(
            body,
            "<g clip-path=\"url(#{id}{slot})\"><g transform=\"translate({})\">{}",
            track.last(),
            track.animate("animateTransform", "transform", total)
        );
        for digit in 0..10 {
            let _ = write!(
                body,
                "<text class=\"{}\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\">{}</text>",
                class,
                x,
                baseline + (digit + 1) as f64 * gap,
                digit
            );
        }
        let _ = writeln!(body, "</g></g>");
    }
}

/// The recording as an animated SVG (SMIL, looping; without animation it shows the last frame).
/// Cells are short thick line segments drawn in layers, one per look, later looks on top. Each
/// layer appears through a clip that follows the automaton: rows are built bottom-up, whole rows
/// at a time; each computed row learns when to light up from the left; the lit cells are always
/// 1, ..., p in reading order. Prime verdicts come in no such order and are grouped by step.
pub fn svg(rec: &Recording) -> String {
    let rows = rec.rows;
    let steps = rec.steps() as usize;
    let computed = BOOTSTRAP_ROWS + 1;
    let pitch = (576 / rows).clamp(2, 16);
    let cell = pitch - 1;
    let (left, top) = (24u64, 72u64);
    let size = pitch * rows;
    let (width, height) = (2 * left + size, top + size + 28);
    let mut start = vec![0.0f64; steps + 2];
    for t in 0..=steps {
        start[t + 1] = start[t]
            + if t == steps {
                HOLD_SECONDS
            } else {
                frame_seconds(t as u64)
            };
    }
    let total = start[steps + 1];
    let history = |x: u64, row: u64| &rec.cells[(row_start(row) + x - 1) as usize];
    let row_top = |row: u64| top + (rows - row) * pitch;
    let segment = |x: u64, row: u64| {
        format!(
            "M{} {}h{}",
            left + x * pitch,
            row_top(row) as f64 + cell as f64 / 2.0,
            cell
        )
    };
    let path_of = |keep: &dyn Fn(u64) -> bool| -> String {
        (1..=rows)
            .filter(|&row| keep(row))
            .flat_map(|row| (0..row).map(move |x| (x, row)))
            .map(|(x, row)| segment(x, row))
            .collect()
    };
    let mut defs = String::new();
    let mut body = String::new();

    // Built rows: table rows know when to light up from the start, computed rows start testing.
    for row in 1..=rows {
        let created = history(0, row).created;
        assert!(
            (0..row).all(|x| history(x, row).created == created),
            "row {} is built whole",
            row
        );
        assert!(row == 1 || history(0, row - 1).created <= created);
    }
    let (mut built_y, mut built_h) = (Track::new(), Track::new());
    for t in 0..=steps {
        let built = (1..=rows)
            .take_while(|&row| history(0, row).created <= t as u64)
            .count() as u64;
        built_y.set(start[t], (top + (rows - built) * pitch).to_string());
        built_h.set(start[t], (built * pitch).to_string());
    }
    let _ = writeln!(
        defs,
        "<clipPath id=\"built\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\">{}{}</rect></clipPath>",
        left,
        built_y.last(),
        size,
        built_h.last(),
        built_y.animate("animate", "y", total),
        built_h.animate("animate", "height", total)
    );
    let _ = writeln!(
        body,
        "<g clip-path=\"url(#built)\"><path stroke=\"{}\" d=\"{}\"/><path stroke=\"{}\" d=\"{}\"/></g>",
        Look::Ready.hex(),
        path_of(&|row| row < computed),
        Look::Testing.hex(),
        path_of(&|row| row >= computed)
    );

    // Prime verdicts, grouped by the step at which they arrive.
    let mut verdicts: BTreeMap<(Look, u64), String> = BTreeMap::new();
    for (index, h) in rec.cells.iter().enumerate() {
        if let Some((at, prime)) = h.verdict {
            let look = if prime { Look::Prime } else { Look::Composite };
            let (x, row) = rec.positions[index];
            verdicts
                .entry((look, at))
                .or_default()
                .push_str(&segment(x, row));
        }
    }
    for ((look, at), d) in &verdicts {
        let mut shown = Track::new();
        shown.set(0.0, "0");
        shown.set(start[*at as usize], "1");
        let _ = writeln!(
            body,
            "<path stroke=\"{}\" d=\"{}\">{}</path>",
            look.hex(),
            d,
            shown.animate("animate", "opacity", total)
        );
    }

    // Computed rows learn when to light up from left to right.
    if rows >= computed {
        let mut rects = String::new();
        for row in computed..=rows {
            let mut known = Track::new();
            for t in 0..=steps {
                let ready: Vec<bool> = (0..row)
                    .map(|x| history(x, row).ready.is_some_and(|at| at <= t as u64))
                    .collect();
                let k = ready.iter().take_while(|&&r| r).count();
                assert!(
                    ready[k..].iter().all(|&r| !r),
                    "row {} learns from the left",
                    row
                );
                known.set(start[t], (k as u64 * pitch).to_string());
            }
            let _ = write!(
                rects,
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\">{}</rect>",
                left,
                row_top(row),
                known.last(),
                pitch,
                known.animate("animate", "width", total)
            );
        }
        let _ = writeln!(defs, "<clipPath id=\"ready\">{}</clipPath>", rects);
        let _ = writeln!(
            body,
            "<path stroke=\"{}\" clip-path=\"url(#ready)\" d=\"{}\"/>",
            Look::Ready.hex(),
            path_of(&|row| row >= computed)
        );
    }

    // Lit cells: the full rows below, and the start of the next row.
    let (mut full_y, mut full_h, mut part_y, mut part_w) =
        (Track::new(), Track::new(), Track::new(), Track::new());
    for t in 0..=steps {
        let count = rec.counts[t];
        let full = (1..=rows)
            .take_while(|&row| row_start(row + 1) - 1 <= count)
            .count() as u64;
        full_y.set(start[t], (top + (rows - full) * pitch).to_string());
        full_h.set(start[t], (full * pitch).to_string());
        if full < rows {
            part_y.set(start[t], row_top(full + 1).to_string());
        }
        part_w.set(
            start[t],
            ((count + 1 - row_start(full + 1)) * pitch).to_string(),
        );
    }
    let _ = writeln!(
        defs,
        "<clipPath id=\"lit\"><rect x=\"{l}\" y=\"{}\" width=\"{}\" height=\"{}\">{}{}</rect><rect x=\"{l}\" y=\"{}\" width=\"{}\" height=\"{}\">{}{}</rect></clipPath>",
        full_y.last(),
        size,
        full_h.last(),
        full_y.animate("animate", "y", total),
        full_h.animate("animate", "height", total),
        part_y.last(),
        part_w.last(),
        pitch,
        part_y.animate("animate", "y", total),
        part_w.animate("animate", "width", total),
        l = left
    );
    let all = path_of(&|_| true);
    let _ = writeln!(
        body,
        "<path stroke=\"{}\" clip-path=\"url(#lit)\" d=\"{}\"/>",
        Look::Lit.hex(),
        all
    );

    // The cells lit in this step flash white: at most three pieces of rows.
    let mut flash: Vec<[Track; 3]> = (0..3)
        .map(|_| [Track::new(), Track::new(), Track::new()])
        .collect();
    for [x, y, w] in flash.iter_mut() {
        x.set(0.0, left.to_string());
        y.set(0.0, top.to_string());
        w.set(0.0, "0");
    }
    for t in 0..=steps {
        let mut pieces: Vec<(u64, u64, u64)> = vec![];
        for n in rec.just_lit(t as u64) {
            let (x, row) = rec.positions[n as usize - 1];
            match pieces.last_mut() {
                Some(piece) if piece.1 == row => piece.2 += 1,
                _ => pieces.push((x, row, 1)),
            }
        }
        assert!(pieces.len() <= flash.len());
        for (i, [x, y, w]) in flash.iter_mut().enumerate() {
            match pieces.get(i) {
                Some(&(px, row, len)) => {
                    x.set(start[t], (left + px * pitch).to_string());
                    y.set(start[t], row_top(row).to_string());
                    w.set(start[t], (len * pitch).to_string());
                }
                None => w.set(start[t], "0"),
            }
            if start[t + 1] - start[t] > FLASH_SECONDS {
                w.set(start[t] + FLASH_SECONDS, "0");
            }
        }
    }
    let mut rects = String::new();
    for [x, y, w] in &flash {
        let _ = write!(
            rects,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\">{}{}{}</rect>",
            x.last(),
            y.last(),
            w.last(),
            pitch,
            x.animate("animate", "x", total),
            y.animate("animate", "y", total),
            w.animate("animate", "width", total)
        );
    }
    let _ = writeln!(defs, "<clipPath id=\"flash\">{}</clipPath>", rects);
    let _ = writeln!(
        body,
        "<path stroke=\"{}\" clip-path=\"url(#flash)\" d=\"{}\"/>",
        FLASH_HEX, all
    );

    // The counters, in the empty corner to the right of the triangle.
    let cx = (left + size * 62 / 100) as f64;
    let cy = (top + size * 47 / 100) as f64;
    let column = cx + 104.0;
    let steps_shown: Vec<(f64, u64)> = (0..=steps).map(|t| (start[t], t as u64)).collect();
    let counts_shown: Vec<(f64, u64)> = (0..=steps).map(|t| (start[t], rec.counts[t])).collect();
    let mut texts = String::new();
    digit_field(
        &mut defs,
        &mut texts,
        "step",
        column,
        cy - 34.0,
        22.0,
        "num",
        3,
        &steps_shown,
        total,
    );
    digit_field(
        &mut defs,
        &mut texts,
        "count",
        column,
        cy + 10.0,
        40.0,
        "big",
        4,
        &counts_shown,
        total,
    );
    let _ = writeln!(
        texts,
        "<text class=\"mid\" x=\"{x}\" y=\"{}\">step</text><text class=\"mid\" x=\"{x}\" y=\"{}\">lit cells</text>",
        cy - 34.0,
        cy + 10.0,
        x = column + 10.0
    );
    let _ = writeln!(
        texts,
        "<text class=\"dim\" x=\"{x}\" y=\"{}\">after step n, the number of lit cells</text><text class=\"dim\" x=\"{x}\" y=\"{}\">is the n-th prime: 2, 3, 5, 7, 11, …</text>",
        cy + 36.0,
        cy + 52.0,
        x = cx
    );
    let mut ly = cy + 90.0;
    let legend = [
        (Look::Lit.hex(), Look::Lit.label()),
        (FLASH_HEX, "lit in this step"),
        (Look::Ready.hex(), Look::Ready.label()),
        (Look::Prime.hex(), Look::Prime.label()),
        (Look::Composite.hex(), Look::Composite.label()),
        (Look::Testing.hex(), Look::Testing.label()),
    ];
    for (colour, label) in legend {
        let _ = writeln!(
            texts,
            "<rect x=\"{}\" y=\"{}\" width=\"11\" height=\"11\" fill=\"{}\"/><text class=\"dim\" x=\"{}\" y=\"{}\">{}</text>",
            cx,
            ly - 10.0,
            colour,
            cx + 18.0,
            ly,
            label
        );
        ly += 20.0;
    }
    if rows >= computed {
        let _ = writeln!(
            texts,
            "<text class=\"dim\" x=\"{x}\" y=\"{}\">rows 1–{} know when to light up</text><text class=\"dim\" x=\"{x}\" y=\"{}\">from a table built into the rule</text>",
            ly + 10.0,
            BOOTSTRAP_ROWS,
            ly + 26.0,
            x = cx
        );
        let _ = writeln!(
            texts,
            "<text class=\"dim\" x=\"{}\" y=\"{}\">← end of the table ({})</text>",
            left + BOOTSTRAP_ROWS * pitch + 6,
            row_top(BOOTSTRAP_ROWS) + cell,
            row_start(computed) - 1
        );
    }
    let _ = writeln!(
        texts,
        "<text class=\"dim\" x=\"{}\" y=\"{}\" text-anchor=\"end\">1</text>",
        left - 5,
        row_top(1) + cell
    );

    let mut s = String::new();
    let _ = writeln!(s, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    let _ = writeln!(
        s,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">",
        w = width,
        h = height
    );
    let _ = writeln!(
        s,
        "<style>text{{font-family:-apple-system,BlinkMacSystemFont,\"Segoe UI\",Helvetica,Arial,sans-serif}}\
         .title{{font-size:19px;font-weight:600;fill:#e6edf3}}.mid{{font-size:14px;fill:#c9d1d9}}.dim{{font-size:12px;fill:#8b949e}}\
         .num{{font-size:22px;font-weight:600;fill:#e6edf3;font-variant-numeric:tabular-nums}}\
         .big{{font-size:40px;font-weight:700;fill:#f2cc60;font-variant-numeric:tabular-nums}}</style>"
    );
    let _ = writeln!(s, "<defs>\n{}</defs>", defs);
    let _ = writeln!(
        s,
        "<rect width=\"{}\" height=\"{}\" rx=\"10\" fill=\"{}\"/>",
        width,
        height,
        Look::Absent.hex()
    );
    let _ = writeln!(
        s,
        "<text class=\"title\" x=\"{}\" y=\"34\">A cellular automaton that counts the primes</text>",
        left
    );
    let _ = writeln!(
        s,
        "<text class=\"dim\" x=\"{}\" y=\"56\">One cell per number, 1 at the bottom. After each step, cells 1 to the next prime are lit.</text>",
        left
    );
    let _ = writeln!(
        s,
        "<g fill=\"none\" stroke-width=\"{}\">\n{}</g>",
        cell, body
    );
    s.push_str(&texts);
    let _ = writeln!(s, "</svg>");
    s
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn the_recording_is_the_automaton_and_every_cell_is_ready_before_it_lights_up() {
        let rec = record(60);
        let primes: Vec<u64> = (2..=row_start(61))
            .filter(|&m| (2..m).take_while(|d| d * d <= m).all(|d| m % d != 0))
            .collect();
        assert_eq!(rec.counts[0], 1);
        for t in 1..=rec.steps() {
            assert_eq!(rec.counts[t as usize], primes[t as usize - 1], "step {}", t);
            let lit = rec
                .cells
                .iter()
                .filter(|h| h.lit.is_some_and(|at| at <= t))
                .count() as u64;
            assert_eq!(lit, rec.counts[t as usize], "step {}", t);
        }
        // The recording stops at the last prime of the rows: the next one is beyond them.
        assert!(primes[rec.steps() as usize] > row_start(61) - 1);
        for row in 1..=rec.rows {
            for x in 0..row {
                let h = rec.cells[(row_start(row) + x - 1) as usize];
                for t in 0..rec.steps() {
                    assert!(rec.look(x, row, t) <= rec.look(x, row, t + 1));
                }
                if row > BOOTSTRAP_ROWS {
                    let verdict = h.verdict.expect("computed cells test divisors").0;
                    let ready = h.ready.expect("computed cells learn their step");
                    assert!(h.created < verdict && verdict < ready);
                    if let Some(lit) = h.lit {
                        assert!(ready < lit, "cell ({}, {}) was not ready in time", x, row);
                    }
                }
            }
        }
    }

    #[test]
    fn every_svg_animation_has_valid_key_times() {
        let svg = svg(&record(56));
        let mut animations = 0;
        for tag in svg.split("<animate").skip(1) {
            let attribute = |name: &str| {
                let from = tag.find(&format!("{}=\"", name)).unwrap() + name.len() + 2;
                tag[from..from + tag[from..].find('"').unwrap()].to_string()
            };
            let times: Vec<f64> = attribute("keyTimes")
                .split(';')
                .map(|v| v.parse().unwrap())
                .collect();
            assert_eq!(times.len(), attribute("values").split(';').count());
            assert_eq!(times[0], 0.0);
            assert!(times.windows(2).all(|w| w[0] < w[1]) && times[times.len() - 1] <= 1.0);
            animations += 1;
        }
        assert!(
            animations > 20 && animations < 400,
            "{} animations",
            animations
        );
        // Every element that is not self-closing is closed.
        for name in ["path", "g", "text"] {
            let open = svg
                .split(&format!("<{} ", name))
                .skip(1)
                .filter(|tag| !tag[..tag.find('>').unwrap()].ends_with('/'))
                .count()
                + svg.matches(&format!("<{}>", name)).count();
            assert_eq!(
                open,
                svg.matches(&format!("</{}>", name)).count(),
                "<{}>",
                name
            );
        }
        assert!(svg.trim_end().ends_with("</svg>"));
    }
}
