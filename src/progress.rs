/*
* Shared, thread-safe search telemetry.
* Workers flush counters here in batches; a UI (the dashboard server or a log line) takes snapshots.
*/

use std::fmt::Write as _;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// The evolution of one rule: configuration of set cells at every time step.
#[derive(Clone, Debug, Default)]
pub struct Preview {
    pub depth: usize,
    pub entries: usize,
    pub counts: Vec<usize>,
    pub frames: Vec<Vec<Vec<i32>>>,
}

#[derive(Default)]
struct Inner {
    label: String,
    phase: String,
    summary: String,
    target: Vec<usize>,
    nodes_by_depth: Vec<u64>,
    solutions: u64,
    deepest: usize,
    jobs_total: u64,
    jobs_done: u64,
    search_started: Option<Instant>,
    search_elapsed: f64,
    running: bool,
    deepest_preview: Option<Preview>,
    solution_previews: Vec<Preview>,
    overall_done: u64,
    overall_total: u64,
    history: Vec<String>,
}

pub struct Progress {
    started: Instant,
    inner: Mutex<Inner>,
}

const MAX_SOLUTION_PREVIEWS: usize = 3;
const MAX_HISTORY: usize = 200;

impl Progress {
    pub fn new() -> Arc<Progress> {
        Arc::new(Progress {
            started: Instant::now(),
            inner: Mutex::new(Inner::default()),
        })
    }

    pub fn begin(&self, label: &str, target: &[usize]) {
        let mut s = self.inner.lock().unwrap();
        s.label = label.to_string();
        s.target = target.to_vec();
        s.nodes_by_depth = vec![0; target.len()];
        s.solutions = 0;
        s.deepest = 0;
        s.jobs_total = 1;
        s.jobs_done = 0;
        s.search_started = Some(Instant::now());
        s.search_elapsed = 0.0;
        s.running = true;
        s.summary.clear();
        s.deepest_preview = None;
        s.solution_previews.clear();
    }

    pub fn finish(&self, summary: &str) {
        let mut s = self.inner.lock().unwrap();
        if let Some(t) = s.search_started {
            s.search_elapsed = t.elapsed().as_secs_f64();
        }
        s.running = false;
        s.summary = summary.to_string();
    }

    pub fn set_phase(&self, phase: &str, done: u64, total: u64) {
        let mut s = self.inner.lock().unwrap();
        s.phase = phase.to_string();
        s.overall_done = done;
        s.overall_total = total;
    }

    pub fn push_history(&self, line: String) {
        let mut s = self.inner.lock().unwrap();
        s.history.push(line);
        if s.history.len() > MAX_HISTORY {
            s.history.remove(0);
        }
    }

    pub fn flush(&self, depth_deltas: &[u64], solutions: u64) {
        let mut s = self.inner.lock().unwrap();
        for (slot, delta) in s.nodes_by_depth.iter_mut().zip(depth_deltas) {
            *slot += *delta;
        }
        s.solutions += solutions;
    }

    pub fn add_jobs(&self, total_delta: i64, finished: u64) {
        let mut s = self.inner.lock().unwrap();
        s.jobs_total = (s.jobs_total as i64 + total_delta).max(0) as u64;
        s.jobs_done += finished;
    }

    /// Records `depth` as the deepest time step reached so far, keeping the evolution that got there.
    pub fn offer_deepest(&self, depth: usize, make: impl FnOnce() -> Preview) {
        let mut s = self.inner.lock().unwrap();
        if s.deepest_preview.is_none() || depth > s.deepest {
            s.deepest = depth;
            s.deepest_preview = Some(make());
        }
    }

    pub fn add_solution_preview(&self, preview: Preview) {
        let mut s = self.inner.lock().unwrap();
        if s.solution_previews.len() < MAX_SOLUTION_PREVIEWS {
            s.solution_previews.push(preview);
        }
    }

    pub fn to_json(&self) -> String {
        let s = self.inner.lock().unwrap();
        let elapsed = match (s.running, s.search_started) {
            (true, Some(t)) => t.elapsed().as_secs_f64(),
            _ => s.search_elapsed,
        };
        let mut out = String::with_capacity(2048);
        out.push('{');
        let _ = write!(out, "\"label\":{},", json_string(&s.label));
        let _ = write!(out, "\"phase\":{},", json_string(&s.phase));
        let _ = write!(out, "\"summary\":{},", json_string(&s.summary));
        let _ = write!(out, "\"running\":{},", s.running);
        let _ = write!(out, "\"elapsed\":{:.3},", elapsed);
        let _ = write!(out, "\"uptime\":{:.3},", self.started.elapsed().as_secs_f64());
        let _ = write!(out, "\"target\":{},", json_numbers(&s.target));
        let _ = write!(out, "\"nodes_by_depth\":{},", json_numbers(&s.nodes_by_depth));
        let _ = write!(out, "\"nodes\":{},", s.nodes_by_depth.iter().sum::<u64>());
        let _ = write!(out, "\"solutions\":{},", s.solutions);
        let _ = write!(out, "\"deepest\":{},", s.deepest);
        let _ = write!(out, "\"jobs_done\":{},\"jobs_total\":{},", s.jobs_done, s.jobs_total);
        let _ = write!(out, "\"overall_done\":{},\"overall_total\":{},", s.overall_done, s.overall_total);
        let history: Vec<String> = s.history.iter().map(|h| json_string(h)).collect();
        let _ = write!(out, "\"history\":[{}],", history.join(","));
        match &s.deepest_preview {
            Some(p) => {
                let _ = write!(out, "\"deepest_preview\":{},", preview_json(p));
            }
            None => out.push_str("\"deepest_preview\":null,"),
        }
        let previews: Vec<String> = s.solution_previews.iter().map(preview_json).collect();
        let _ = write!(out, "\"solution_previews\":[{}]", previews.join(","));
        out.push('}');
        out
    }
}

fn preview_json(p: &Preview) -> String {
    let frames: Vec<String> = p
        .frames
        .iter()
        .map(|frame| {
            let cells: Vec<String> = frame.iter().map(|c| json_numbers(c)).collect();
            format!("[{}]", cells.join(","))
        })
        .collect();
    format!(
        "{{\"depth\":{},\"entries\":{},\"counts\":{},\"frames\":[{}]}}",
        p.depth,
        p.entries,
        json_numbers(&p.counts),
        frames.join(",")
    )
}

fn json_numbers<T: std::fmt::Display>(values: &[T]) -> String {
    let parts: Vec<String> = values.iter().map(|v| v.to_string()).collect();
    format!("[{}]", parts.join(","))
}

pub fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn json_snapshot_is_well_formed() {
        let progress = Progress::new();
        progress.begin("demo \"quoted\"", &[1, 2, 3]);
        progress.flush(&[1, 4, 0], 2);
        progress.add_jobs(3, 1);
        progress.offer_deepest(1, || Preview {
            depth: 1,
            entries: 5,
            counts: vec![1, 2],
            frames: vec![vec![vec![0, 0]], vec![vec![0, 0], vec![1, 0]]],
        });
        progress.push_history("line\nbreak".to_string());
        let json = progress.to_json();
        assert!(json.starts_with('{') && json.ends_with('}'));
        assert!(json.contains("\"label\":\"demo \\\"quoted\\\"\""));
        assert!(json.contains("\"nodes_by_depth\":[1,4,0]"));
        assert!(json.contains("\"nodes\":5"));
        assert!(json.contains("\"jobs_total\":4"));
        assert!(json.contains("\"frames\":[[[0,0]],[[0,0],[1,0]]]"));
        assert!(json.contains("line\\nbreak"));
    }

    #[test]
    fn deepest_only_moves_forward() {
        let progress = Progress::new();
        progress.begin("x", &[1, 2, 3, 5]);
        progress.offer_deepest(2, || Preview { depth: 2, ..Default::default() });
        progress.offer_deepest(1, || panic!("shallower previews must not be built"));
        assert!(progress.to_json().contains("\"deepest\":2"));
    }
}
