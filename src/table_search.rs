/*
* Lazy rule-table search
* ======================
* A rule is a pair of lookup tables over neighbourhood patterns: BIRTH (cell currently unset) and
* SURVIVAL (cell currently set). The pattern of a cell is the bit mask of which neighbours are set.
* The CA starts from one set cell and must have exactly target[t] set cells at every time t.
*
* Instead of enumerating formulas, the search simulates the automaton and only decides a table entry
* the first time the simulation needs it (the "consulted" patterns). Every node of the search tree is
* therefore one distinct trajectory prefix, and a partial table stands for every full rule that agrees
* with it. The reductions, all exact:
*
*  1. Prefix pruning     a step must produce exactly target[t+1] set cells, so only combinations of
*                        new table entries that sum to the required count are generated.
*  2. Lazy tables        unconsulted entries stay free; one failing simulation discards a whole family.
*  3. Quiescent space    cells with no set neighbour stay unset, so the all-zero birth entry never
*                        exists. The cell itself is not a table input (it selects the table).
*  4. Symmetry           lattice symmetries that preserve the neighbourhood map a rule to a rule whose
*                        evolution is the mirror image, with identical counts. Only the trajectory
*                        prefix that is lexicographically smallest in its orbit is explored.
*  5. Complexity bound   optionally limit the number of consulted entries (simplest rules first).
*/

use std::cmp::Ordering as CmpOrdering;
use std::collections::{BTreeMap, HashMap};
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rayon::prelude::*;

use crate::lattice::{Lattice, Neighbourhood, SymmetryGroup, MAX_NEIGHBOURS};
use crate::progress::{Preview, Progress};

/// Bit 63 of a scatter entry marks "this cell is itself set"; pattern masks use bits 0..62.
const SELF_BIT: u64 = 1 << 63;
const DENSE_LIMIT: usize = 20;
/// Search-tree levels shallower than this are split across threads; deeper subtrees run sequentially.
const PARALLEL_DEPTH: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurvivalMode {
    /// Survival is a free table (cells may die).
    Free,
    /// Set cells never die: monotone growth, only births are searched.
    Always,
    /// Set cells always die after one step.
    Never,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    FoundFirst,
    TimeLimit,
    NodeLimit,
}

#[derive(Clone)]
pub struct SearchParams {
    pub neighbourhood: Neighbourhood,
    pub target: Vec<usize>,
    pub survival: SurvivalMode,
    pub symmetry: bool,
    pub find_first: bool,
    pub store_limit: usize,
    pub max_entries: Option<usize>,
    pub time_limit: Option<Duration>,
    pub node_limit: Option<u64>,
    pub parallel: bool,
    pub threads: Option<usize>,
    pub progress: Option<Arc<Progress>>,
    pub label: String,
}

impl SearchParams {
    pub fn new(neighbourhood: Neighbourhood, target: Vec<usize>) -> Self {
        SearchParams {
            neighbourhood,
            target,
            survival: SurvivalMode::Free,
            symmetry: true,
            find_first: false,
            store_limit: 8,
            max_entries: None,
            time_limit: None,
            node_limit: None,
            parallel: true,
            threads: None,
            progress: None,
            label: String::new(),
        }
    }
}

/// The table entries a solution consulted. Every other entry is a don't-care.
#[derive(Clone, Debug)]
pub struct Solution {
    pub birth: Vec<(u64, bool)>,
    pub survival: Vec<(u64, bool)>,
    /// Number of distinct rules in the symmetry orbit this representative stands for.
    pub orbit: usize,
}

#[derive(Debug, Clone)]
pub struct SearchReport {
    pub stop_reason: Option<StopReason>,
    pub nodes: u64,
    pub nodes_by_depth: Vec<u64>,
    pub solutions: u64,
    /// For each solution cylinder: free (unconsulted) entries -> sum of orbit sizes.
    /// The number of distinct full rules matching the target is the sum of weight * 2^free.
    pub weighted: BTreeMap<u64, u128>,
    pub stored: Vec<Solution>,
    /// Deepest time step reached; deepest + 1 is the longest matched prefix of the target.
    pub deepest: usize,
    pub elapsed: Duration,
}

impl SearchReport {
    pub fn complete(&self) -> bool {
        self.stop_reason.is_none()
    }

    pub fn matched_terms(&self) -> usize {
        self.deepest + 1
    }

    /// log2 of the number of rules in the unrestricted space for `k` neighbours.
    pub fn space_bits(k: usize, mode: SurvivalMode) -> u64 {
        let birth = (1u64 << k) - 1;
        match mode {
            SurvivalMode::Free => birth + (1u64 << k),
            _ => birth,
        }
    }

    pub fn exact_rule_count(&self) -> Option<u128> {
        let mut total: u128 = 0;
        for (&free, &weight) in &self.weighted {
            if free > 100 {
                return None;
            }
            total = total.checked_add(weight.checked_mul(1u128 << free)?)?;
        }
        Some(total)
    }

    pub fn log2_rule_count(&self) -> Option<f64> {
        let max_free = *self.weighted.keys().max()? as f64;
        let sum: f64 = self
            .weighted
            .iter()
            .map(|(&free, &weight)| (weight as f64) * (free as f64 - max_free).exp2())
            .sum();
        Some(max_free + sum.log2())
    }
}

#[derive(Default)]
struct FxHasher(u64);

impl Hasher for FxHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u64(b as u64);
        }
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

type FxMap = HashMap<u64, u8, BuildHasherDefault<FxHasher>>;

/// 0 = unassigned, 1 = assigned off, 2 = assigned on.
enum Store {
    Dense(Vec<u8>),
    Sparse(FxMap),
}

impl Store {
    fn new(k: usize) -> Store {
        if k <= DENSE_LIMIT {
            Store::Dense(vec![0; 1usize << k])
        } else {
            Store::Sparse(FxMap::default())
        }
    }

    #[inline]
    fn get(&self, mask: u64) -> u8 {
        match self {
            Store::Dense(v) => v[mask as usize],
            Store::Sparse(m) => m.get(&mask).copied().unwrap_or(0),
        }
    }

    #[inline]
    fn set(&mut self, mask: u64, value: u8) {
        match self {
            Store::Dense(v) => v[mask as usize] = value,
            Store::Sparse(m) => {
                if value == 0 {
                    m.remove(&mask);
                } else {
                    m.insert(mask, value);
                }
            }
        }
    }
}

#[inline]
fn lookup_in(birth: &Store, survive: &Store, mode: SurvivalMode, is_set: bool, mask: u64) -> u8 {
    if !is_set {
        return birth.get(mask);
    }
    match mode {
        SurvivalMode::Always => 2,
        SurvivalMode::Never => 1,
        SurvivalMode::Free => survive.get(mask),
    }
}

struct Ctx {
    k: usize,
    deltas: Vec<i64>,
    target: Vec<usize>,
    survival: SurvivalMode,
    lattice: Lattice,
    group: SymmetryGroup,
    total_entries: u64,
    max_entries: usize,
    store_limit: usize,
    find_first: bool,
    deadline: Option<Instant>,
    node_limit: u64,
    node_counter: AtomicU64,
    stop: AtomicBool,
    stop_reason: Mutex<Option<StopReason>>,
    progress: Option<Arc<Progress>>,
}

impl Ctx {
    fn new(params: &SearchParams) -> Result<Ctx, String> {
        let nb = &params.neighbourhood;
        let target = &params.target;
        if target.first() != Some(&1) {
            return Err("the target must start with 1 (the single seed cell)".to_string());
        }
        let k = nb.len();
        if k > MAX_NEIGHBOURS {
            return Err(format!("{} neighbours exceed the supported maximum of {}", k, MAX_NEIGHBOURS));
        }
        let extent = nb.radius() * (target.len() as i32 + 1) + 1;
        let lattice = Lattice::new(nb.dim, extent)?;
        let group = if params.symmetry {
            SymmetryGroup::preserving(nb)
        } else {
            SymmetryGroup::trivial()
        };
        let birth_entries = (1u64 << k) - 1;
        let survival_entries = if params.survival == SurvivalMode::Free { 1u64 << k } else { 0 };
        Ok(Ctx {
            k,
            deltas: nb.offsets.iter().map(|o| lattice.delta(o)).collect(),
            target: target.clone(),
            survival: params.survival,
            lattice,
            group,
            total_entries: birth_entries + survival_entries,
            max_entries: params.max_entries.unwrap_or(usize::MAX),
            store_limit: params.store_limit,
            find_first: params.find_first,
            deadline: params.time_limit.map(|d| Instant::now() + d),
            node_limit: params.node_limit.unwrap_or(u64::MAX),
            node_counter: AtomicU64::new(0),
            stop: AtomicBool::new(false),
            stop_reason: Mutex::new(None),
            progress: params.progress.clone(),
        })
    }

    fn request_stop(&self, reason: StopReason) {
        let mut guard = self.stop_reason.lock().unwrap();
        if guard.is_none() {
            *guard = Some(reason);
        }
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[derive(Clone, Copy)]
struct Front {
    cell: i64,
    mask: u64,
    is_set: bool,
}

/// All frontier cells with the same (is_set, mask), i.e. the same unassigned table entry.
#[derive(Clone, Copy)]
struct Group {
    is_set: bool,
    mask: u64,
    count: u32,
}

#[derive(Default)]
struct Level {
    cells: Vec<i64>,
    scatter: Vec<(i64, u64)>,
    frontier: Vec<Front>,
    groups: Vec<Group>,
    suffix: Vec<u32>,
    need: u32,
}

/// A search-tree node that can be rebuilt on any thread.
struct Job {
    t: usize,
    path: Vec<Vec<i64>>,
    stab: Vec<u16>,
    trail: Vec<(bool, u64, bool)>,
}

struct Stats {
    nodes_by_depth: Vec<u64>,
    solutions: u64,
    weighted: BTreeMap<u64, u128>,
    stored: Vec<Solution>,
}

struct Searcher<'a> {
    ctx: &'a Ctx,
    birth: Store,
    survive: Store,
    /// Entries assigned on the path from the root, in order: (is_set, mask, value).
    trail: Vec<(bool, u64, bool)>,
    levels: Vec<Level>,
    collect: Option<Vec<Job>>,
    pending: Vec<(bool, u64)>,
    image: Vec<i64>,
    local_nodes: u64,
    next_check: u64,
    nodes_by_depth: Vec<u64>,
    flushed_by_depth: Vec<u64>,
    solutions: u64,
    flushed_solutions: u64,
    weighted: BTreeMap<u64, u128>,
    stored: Vec<Solution>,
    deepest: usize,
}

impl<'a> Searcher<'a> {
    fn new(ctx: &'a Ctx) -> Self {
        let depth = ctx.target.len();
        Searcher {
            ctx,
            birth: Store::new(ctx.k),
            survive: Store::new(ctx.k),
            trail: vec![],
            levels: (0..depth).map(|_| Level::default()).collect(),
            collect: None,
            pending: vec![],
            image: vec![],
            local_nodes: 0,
            next_check: 1024,
            nodes_by_depth: vec![0; depth],
            flushed_by_depth: vec![0; depth],
            solutions: 0,
            flushed_solutions: 0,
            weighted: BTreeMap::new(),
            stored: vec![],
            deepest: 0,
        }
    }

    fn into_stats(mut self) -> Stats {
        self.flush_progress();
        Stats {
            nodes_by_depth: self.nodes_by_depth,
            solutions: self.solutions,
            weighted: self.weighted,
            stored: self.stored,
        }
    }

    fn assign(&mut self, is_set: bool, mask: u64, value: bool) {
        let code = if value { 2 } else { 1 };
        if is_set {
            self.survive.set(mask, code);
        } else {
            self.birth.set(mask, code);
        }
        self.trail.push((is_set, mask, value));
    }

    fn unassign(&mut self) {
        let (is_set, mask, _) = self.trail.pop().expect("trail underflow");
        if is_set {
            self.survive.set(mask, 0);
        } else {
            self.birth.set(mask, 0);
        }
    }

    fn load(&mut self, job: &Job) {
        while !self.trail.is_empty() {
            self.unassign();
        }
        for &(is_set, mask, value) in &job.trail {
            self.assign(is_set, mask, value);
        }
        for (level, cells) in self.levels.iter_mut().zip(&job.path) {
            level.cells.clear();
            level.cells.extend_from_slice(cells);
        }
    }

    fn run(&mut self, job: &Job) {
        self.load(job);
        self.visit(job.t, &job.stab);
    }

    fn expand(&mut self, job: &Job) -> Vec<Job> {
        self.load(job);
        self.collect = Some(vec![]);
        self.visit(job.t, &job.stab);
        self.collect.take().unwrap_or_default()
    }

    fn preview(&self, t: usize) -> Preview {
        let dim = self.ctx.lattice.dim();
        let mut counts = vec![];
        let mut frames = vec![];
        for level in &self.levels[..=t] {
            counts.push(level.cells.len());
            let mut frame = vec![];
            for &cell in &level.cells {
                let mut coordinates = vec![0; dim];
                self.ctx.lattice.decode_into(cell, &mut coordinates);
                frame.push(coordinates);
            }
            frames.push(frame);
        }
        Preview { depth: t, entries: self.trail.len(), counts, frames }
    }

    fn flush_progress(&mut self) {
        let ctx = self.ctx;
        if let Some(progress) = &ctx.progress {
            let deltas: Vec<u64> = self
                .nodes_by_depth
                .iter()
                .zip(&self.flushed_by_depth)
                .map(|(now, before)| now - before)
                .collect();
            progress.flush(&deltas, self.solutions - self.flushed_solutions);
            self.flushed_by_depth.copy_from_slice(&self.nodes_by_depth);
            self.flushed_solutions = self.solutions;
        }
    }

    fn note_node(&mut self, t: usize) {
        self.nodes_by_depth[t] += 1;
        self.local_nodes += 1;
        if t > self.deepest {
            self.deepest = t;
            let ctx = self.ctx;
            if let Some(progress) = &ctx.progress {
                progress.offer_deepest(t, || self.preview(t));
            }
        }
        if self.local_nodes >= self.next_check {
            self.next_check += 1024;
            self.checkpoint();
        }
    }

    /// Every 1024 nodes: publish telemetry and enforce the time and node limits.
    fn checkpoint(&mut self) {
        let ctx = self.ctx;
        self.flush_progress();
        if let Some(deadline) = ctx.deadline {
            if Instant::now() >= deadline {
                ctx.request_stop(StopReason::TimeLimit);
            }
        }
        if ctx.node_counter.fetch_add(1024, Ordering::Relaxed) + 1024 > ctx.node_limit {
            ctx.request_stop(StopReason::NodeLimit);
        }
    }

    /// The configuration at depth `t` is in `levels[t].cells`; its predecessors are on the path.
    fn visit(&mut self, t: usize, stab: &[u16]) {
        self.note_node(t);
        if t + 1 == self.ctx.target.len() {
            self.record_solution(t, stab);
            return;
        }
        if self.ctx.stop.load(Ordering::Relaxed) || !self.prepare(t) {
            return;
        }
        let need = self.levels[t].need;
        self.choose(t, 0, need, stab);
    }

    /// Builds the frontier of level `t` (set cells and cells with a set neighbour, each with its
    /// pattern), classifies it against the assigned entries and sums up what is still undecided.
    /// Returns false when the target count for t + 1 can no longer be met.
    fn prepare(&mut self, t: usize) -> bool {
        let ctx = self.ctx;
        let mut level = std::mem::take(&mut self.levels[t]);

        level.scatter.clear();
        for &cell in &level.cells {
            level.scatter.push((cell, SELF_BIT));
            for (j, &delta) in ctx.deltas.iter().enumerate() {
                level.scatter.push((cell - delta, 1u64 << j));
            }
        }
        level.scatter.sort_unstable_by_key(|entry| entry.0);

        level.frontier.clear();
        let mut i = 0;
        while i < level.scatter.len() {
            let cell = level.scatter[i].0;
            let mut bits = 0u64;
            while i < level.scatter.len() && level.scatter[i].0 == cell {
                bits |= level.scatter[i].1;
                i += 1;
            }
            level.frontier.push(Front {
                cell,
                mask: bits & !SELF_BIT,
                is_set: bits & SELF_BIT != 0,
            });
        }

        let mut known = 0usize;
        self.pending.clear();
        for front in &level.frontier {
            match lookup_in(&self.birth, &self.survive, ctx.survival, front.is_set, front.mask) {
                2 => known += 1,
                1 => {}
                _ => self.pending.push((front.is_set, front.mask)),
            }
        }
        self.pending.sort_unstable();
        level.groups.clear();
        for &(is_set, mask) in &self.pending {
            match level.groups.last_mut() {
                Some(group) if group.is_set == is_set && group.mask == mask => group.count += 1,
                _ => level.groups.push(Group { is_set, mask, count: 1 }),
            }
        }

        let target = ctx.target[t + 1];
        let feasible = known <= target && {
            level.need = (target - known) as u32;
            level.suffix.clear();
            level.suffix.resize(level.groups.len() + 1, 0);
            for g in (0..level.groups.len()).rev() {
                level.suffix[g] = level.suffix[g + 1] + level.groups[g].count;
            }
            level.suffix[0] >= level.need
        };
        self.levels[t] = level;
        feasible
    }

    /// Decides the undecided entries of level `t` one group at a time so that exactly `remaining`
    /// more cells become set.
    fn choose(&mut self, t: usize, i: usize, remaining: u32, stab: &[u16]) {
        let (group, suffix, group_count) = {
            let level = &self.levels[t];
            if i == level.groups.len() {
                (None, 0, 0)
            } else {
                (Some(level.groups[i]), level.suffix[i], level.groups.len())
            }
        };
        let group = match group {
            Some(group) => group,
            None => {
                if remaining == 0 {
                    self.emit_child(t, stab);
                }
                return;
            }
        };
        let _ = group_count;
        if suffix < remaining || self.ctx.stop.load(Ordering::Relaxed) {
            return;
        }
        if self.trail.len() >= self.ctx.max_entries {
            return;
        }
        if group.count <= remaining {
            self.assign(group.is_set, group.mask, true);
            self.choose(t, i + 1, remaining - group.count, stab);
            self.unassign();
        }
        self.assign(group.is_set, group.mask, false);
        self.choose(t, i + 1, remaining, stab);
        self.unassign();
    }

    /// All entries of level `t` are decided: derive the next configuration, apply the symmetry
    /// test and either recurse or (when splitting work across threads) queue a job.
    fn emit_child(&mut self, t: usize, stab: &[u16]) {
        let ctx = self.ctx;
        {
            let (head, tail) = self.levels.split_at_mut(t + 1);
            let current = &head[t];
            let next = &mut tail[0];
            next.cells.clear();
            for front in &current.frontier {
                if lookup_in(&self.birth, &self.survive, ctx.survival, front.is_set, front.mask) == 2 {
                    next.cells.push(front.cell);
                }
            }
        }
        debug_assert_eq!(self.levels[t + 1].cells.len(), ctx.target[t + 1]);

        // Lexicographic leader test on the trajectory prefix. `stab` holds the symmetries that fix
        // every earlier configuration; only those can still produce a smaller image.
        let mut child_stab: Vec<u16> = Vec::new();
        if !stab.is_empty() {
            let cells = &self.levels[t + 1].cells;
            for &element in stab {
                self.image.clear();
                self.image
                    .extend(cells.iter().map(|&c| ctx.group.apply_cell(&ctx.lattice, element as usize, c)));
                self.image.sort_unstable();
                match self.image.as_slice().cmp(cells.as_slice()) {
                    CmpOrdering::Less => return,
                    CmpOrdering::Equal => child_stab.push(element),
                    CmpOrdering::Greater => {}
                }
            }
        }

        if self.collect.is_some() && t + 2 < ctx.target.len() {
            let job = Job {
                t: t + 1,
                path: (0..=t + 1).map(|i| self.levels[i].cells.clone()).collect(),
                stab: child_stab,
                trail: self.trail.clone(),
            };
            self.collect.as_mut().unwrap().push(job);
        } else {
            self.visit(t + 1, &child_stab);
        }
    }

    fn record_solution(&mut self, t: usize, stab: &[u16]) {
        let ctx = self.ctx;
        self.solutions += 1;
        let orbit = ctx.group.order() / (1 + stab.len());
        let free = ctx.total_entries - self.trail.len() as u64;
        *self.weighted.entry(free).or_insert(0) += orbit as u128;

        if self.stored.len() < ctx.store_limit {
            let mut solution = Solution { birth: vec![], survival: vec![], orbit };
            for &(is_set, mask, value) in &self.trail {
                if is_set {
                    solution.survival.push((mask, value));
                } else {
                    solution.birth.push((mask, value));
                }
            }
            solution.birth.sort_unstable();
            solution.survival.sort_unstable();
            self.stored.push(solution);
            if let Some(progress) = &ctx.progress {
                progress.add_solution_preview(self.preview(t));
            }
        }
        if ctx.find_first {
            ctx.request_stop(StopReason::FoundFirst);
        }
    }
}

fn with_searcher<'a, R>(pool: &[Mutex<Searcher<'a>>], f: impl FnOnce(&mut Searcher<'a>) -> R) -> R {
    let index = rayon::current_thread_index()
        .map(|i| i + 1)
        .unwrap_or(0)
        .min(pool.len() - 1);
    let mut guard = pool[index].lock().unwrap();
    f(&mut guard)
}

/// Splits the top of the tree into jobs on the thread pool; deeper subtrees run sequentially.
fn par_visit(ctx: &Ctx, pool: &[Mutex<Searcher<'_>>], job: Job) {
    if ctx.stop.load(Ordering::Relaxed) {
        return;
    }
    if job.t >= PARALLEL_DEPTH || job.t + 1 >= ctx.target.len() {
        with_searcher(pool, |searcher| searcher.run(&job));
        if let Some(progress) = &ctx.progress {
            progress.add_jobs(0, 1);
        }
        return;
    }
    let children = with_searcher(pool, |searcher| searcher.expand(&job));
    if let Some(progress) = &ctx.progress {
        progress.add_jobs(children.len() as i64 - 1, 0);
    }
    children.into_par_iter().for_each(|child| par_visit(ctx, pool, child));
}

fn merge_stats(all: Vec<Stats>, depth: usize) -> Stats {
    let mut merged = Stats {
        nodes_by_depth: vec![0; depth],
        solutions: 0,
        weighted: BTreeMap::new(),
        stored: vec![],
    };
    for stats in all {
        for (slot, value) in merged.nodes_by_depth.iter_mut().zip(&stats.nodes_by_depth) {
            *slot += value;
        }
        merged.solutions += stats.solutions;
        for (free, weight) in stats.weighted {
            *merged.weighted.entry(free).or_insert(0) += weight;
        }
        merged.stored.extend(stats.stored);
    }
    merged
}

pub fn search(params: &SearchParams) -> Result<SearchReport, String> {
    let ctx = Ctx::new(params)?;
    let started = Instant::now();
    if let Some(progress) = &ctx.progress {
        progress.begin(&params.label, &params.target);
    }
    let root = Job {
        t: 0,
        path: vec![vec![ctx.lattice.origin()]],
        stab: (0..ctx.group.elements.len() as u16).collect(),
        trail: vec![],
    };
    let depth = ctx.target.len();

    let stats = if params.parallel && depth > 1 {
        let threads = params.threads.unwrap_or_else(rayon::current_num_threads).max(1);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .stack_size(64 << 20)
            .build()
            .map_err(|e| e.to_string())?;
        let searchers: Vec<Mutex<Searcher>> = (0..=threads).map(|_| Mutex::new(Searcher::new(&ctx))).collect();
        pool.install(|| par_visit(&ctx, &searchers, root));
        let all = searchers
            .into_iter()
            .map(|m| m.into_inner().unwrap().into_stats())
            .collect();
        merge_stats(all, depth)
    } else {
        let mut searcher = Searcher::new(&ctx);
        searcher.run(&root);
        merge_stats(vec![searcher.into_stats()], depth)
    };

    let stop_reason = *ctx.stop_reason.lock().unwrap();
    let deepest = stats.nodes_by_depth.iter().rposition(|&n| n > 0).unwrap_or(0);
    let mut stored = stats.stored;
    stored.truncate(params.store_limit);
    let report = SearchReport {
        stop_reason,
        nodes: stats.nodes_by_depth.iter().sum(),
        nodes_by_depth: stats.nodes_by_depth,
        solutions: stats.solutions,
        weighted: stats.weighted,
        stored,
        deepest,
        elapsed: started.elapsed(),
    };
    if let Some(progress) = &ctx.progress {
        let summary = format!(
            "{}: {} nodes, {} solution classes, longest matched prefix {} terms",
            if report.complete() { "complete" } else { "stopped early" },
            report.nodes,
            report.solutions,
            report.matched_terms()
        );
        progress.finish(&summary);
    }
    Ok(report)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::reference::{verify_solution, RuleFn};
    use crate::sequences::primes_target;

    fn quick(nb: Neighbourhood, target: Vec<usize>) -> SearchReport {
        let mut params = SearchParams::new(nb, target);
        params.parallel = false;
        search(&params).unwrap()
    }

    #[test]
    fn counts_one_dimensional_growth() {
        let report = quick(Neighbourhood::moore(1, 1).unwrap(), vec![1, 2, 3, 4, 5, 6]);
        assert!(report.complete());
        assert!(report.solutions > 0);
        assert_eq!(report.matched_terms(), 6);
    }

    #[test]
    fn one_dimensional_primes_stop_early() {
        let report = quick(Neighbourhood::moore(1, 1).unwrap(), primes_target(10));
        assert_eq!(report.solutions, 0);
        assert!(report.matched_terms() < 10);
    }

    #[test]
    fn stored_solutions_verify() {
        let nb = Neighbourhood::moore(1, 1).unwrap();
        let target = vec![1, 2, 3, 4, 5];
        let report = quick(nb.clone(), target.clone());
        assert!(!report.stored.is_empty());
        for solution in &report.stored {
            assert!(verify_solution(&nb, &target, solution, SurvivalMode::Free, 8));
        }
    }

    // ------------------------------------------------------------------------------------
    // Exactness: the search must agree with exhaustive enumeration of every full rule.
    // ------------------------------------------------------------------------------------

    /// A full rule as bits: birth entries for masks 1..2^k (mask 0 never occurs), followed by the
    /// survival entries for masks 0..2^k when survival is free.
    struct BitRule {
        k: usize,
        bits: u64,
        mode: SurvivalMode,
    }

    impl RuleFn for BitRule {
        fn birth(&self, mask: u64) -> bool {
            mask != 0 && (self.bits >> (mask - 1)) & 1 == 1
        }

        fn survive(&self, mask: u64) -> bool {
            match self.mode {
                SurvivalMode::Always => true,
                SurvivalMode::Never => false,
                SurvivalMode::Free => (self.bits >> ((1u64 << self.k) - 1 + mask)) & 1 == 1,
            }
        }
    }

    /// Bounded-grid simulation: a third formulation, independent of the engine and of the
    /// reference simulator, fast enough to enumerate tens of thousands of rules.
    struct Grid {
        cells: usize,
        k: usize,
        neighbours: Vec<usize>,
        origin: usize,
    }

    impl Grid {
        fn new(nb: &Neighbourhood, steps: usize) -> Grid {
            let dim = nb.dim;
            let half = nb.radius() as usize * (steps + 1);
            let side = 2 * half + 1;
            let cells = side.pow(dim as u32);
            let k = nb.len();
            let mut neighbours = vec![usize::MAX; cells * k];
            for cell in 0..cells {
                let mut coords = vec![0i64; dim];
                let mut rest = cell;
                for c in coords.iter_mut() {
                    *c = (rest % side) as i64;
                    rest /= side;
                }
                for (j, offset) in nb.offsets.iter().enumerate() {
                    let mut index = 0usize;
                    let mut stride = 1usize;
                    let mut inside = true;
                    for d in 0..dim {
                        let x = coords[d] + offset[d] as i64;
                        if x < 0 || x >= side as i64 {
                            inside = false;
                            break;
                        }
                        index += x as usize * stride;
                        stride *= side;
                    }
                    if inside {
                        neighbours[cell * k + j] = index;
                    }
                }
            }
            let origin = (0..dim).map(|d| half * side.pow(d as u32)).sum();
            Grid { cells, k, neighbours, origin }
        }

        fn matches(&self, rule: &dyn RuleFn, target: &[usize]) -> bool {
            if target[0] != 1 {
                return false;
            }
            let mut current = vec![false; self.cells];
            let mut next = vec![false; self.cells];
            current[self.origin] = true;
            for &expected in &target[1..] {
                let mut count = 0;
                for cell in 0..self.cells {
                    let mut mask = 0u64;
                    for j in 0..self.k {
                        let n = self.neighbours[cell * self.k + j];
                        if n != usize::MAX && current[n] {
                            mask |= 1 << j;
                        }
                    }
                    let alive = if current[cell] { rule.survive(mask) } else { mask != 0 && rule.birth(mask) };
                    next[cell] = alive;
                    count += alive as usize;
                }
                if count != expected {
                    return false;
                }
                std::mem::swap(&mut current, &mut next);
            }
            true
        }
    }

    fn brute_force_count(nb: &Neighbourhood, target: &[usize], mode: SurvivalMode) -> u128 {
        let k = nb.len();
        let total = SearchReport::space_bits(k, mode) as u32;
        assert!(total <= 24, "brute force is only for tiny rule spaces");
        let grid = Grid::new(nb, target.len() - 1);
        (0..(1u64 << total))
            .filter(|&bits| grid.matches(&BitRule { k, bits, mode }, target))
            .count() as u128
    }

    fn run(nb: &Neighbourhood, target: &[usize], mode: SurvivalMode, symmetry: bool, parallel: bool) -> SearchReport {
        let mut params = SearchParams::new(nb.clone(), target.to_vec());
        params.survival = mode;
        params.symmetry = symmetry;
        params.parallel = parallel;
        params.threads = Some(3);
        params.store_limit = 0;
        search(&params).unwrap()
    }

    fn assert_exact(nb: &Neighbourhood, targets: &[Vec<usize>], modes: &[SurvivalMode]) {
        for target in targets {
            for &mode in modes {
                let expected = brute_force_count(nb, target, mode);
                for symmetry in [true, false] {
                    for parallel in [false, true] {
                        let report = run(nb, target, mode, symmetry, parallel);
                        assert!(report.complete());
                        assert_eq!(
                            report.exact_rule_count().unwrap(),
                            expected,
                            "{} neighbours, target {:?}, {:?}, symmetry {}, parallel {}",
                            nb.len(),
                            target,
                            mode,
                            symmetry,
                            parallel
                        );
                    }
                }
            }
        }
    }

    const ALL_MODES: [SurvivalMode; 3] = [SurvivalMode::Free, SurvivalMode::Always, SurvivalMode::Never];

    fn sample_targets() -> Vec<Vec<usize>> {
        vec![
            vec![1, 2, 3, 4, 5, 6],
            vec![1, 2, 3, 5, 7, 11],
            vec![1, 3, 5, 7, 9],
            vec![1, 2, 4, 8],
            vec![1, 2, 3, 5],
            vec![1, 1, 1, 1],
            vec![1, 2, 2, 3, 3, 4],
        ]
    }

    #[test]
    fn exact_against_brute_force_one_dimension() {
        // Two neighbours, symmetry group of order 2 (reflection), all 2^7 rules enumerated.
        assert_exact(&Neighbourhood::moore(1, 1).unwrap(), &sample_targets(), &ALL_MODES);
    }

    #[test]
    fn exact_against_brute_force_asymmetric_neighbourhoods() {
        // Trivial symmetry group.
        let skew = Neighbourhood::new(1, vec![vec![1], vec![-1], vec![2]]).unwrap();
        assert_exact(&skew, &sample_targets()[..4], &ALL_MODES);
    }

    #[test]
    fn exact_against_brute_force_with_reflection_symmetry() {
        // Three neighbours in the plane; a reflection / axis swap of order 2 preserves each set.
        let tee = Neighbourhood::new(2, vec![vec![1, 0], vec![-1, 0], vec![0, 1]]).unwrap();
        let causal = Neighbourhood::new(2, vec![vec![1, 0], vec![0, 1], vec![1, 1]]).unwrap();
        assert_eq!(SymmetryGroup::preserving(&tee).order(), 2);
        assert_eq!(SymmetryGroup::preserving(&causal).order(), 2);
        assert_exact(&tee, &sample_targets()[..4], &ALL_MODES);
        assert_exact(&causal, &sample_targets()[..4], &ALL_MODES);
    }

    #[test]
    fn exact_against_brute_force_full_square_symmetry() {
        // Von Neumann in the plane: the group of the square (order 8) with every orbit size 1, 2, 4, 8.
        let vn = Neighbourhood::von_neumann(2).unwrap();
        assert_eq!(SymmetryGroup::preserving(&vn).order(), 8);
        let modes = [SurvivalMode::Always, SurvivalMode::Never];
        assert_exact(&vn, &sample_targets()[..4], &modes);
        assert_exact(&Neighbourhood::moore(1, 2).unwrap(), &sample_targets()[..4], &modes);
    }

    #[test]
    #[ignore = "enumerates all 2^31 rules of the planar von Neumann class; run with --ignored in release mode"]
    fn exhaustive_planar_von_neumann_class_matches_search() {
        use rayon::prelude::*;
        let nb = Neighbourhood::von_neumann(2).unwrap();
        for target in [vec![1, 2, 3, 5], vec![1, 2, 3, 5, 7]] {
            let grid = Grid::new(&nb, target.len() - 1);
            let brute = (0..(1u64 << 31))
                .into_par_iter()
                .filter(|&bits| grid.matches(&BitRule { k: 4, bits, mode: SurvivalMode::Free }, &target))
                .count() as u128;
            let report = run(&nb, &target, SurvivalMode::Free, true, true);
            assert_eq!(report.exact_rule_count().unwrap(), brute, "{:?}", target);
            println!("{:?}: {} of 2^31 rules match", target, brute);
        }
    }

    #[test]
    fn grid_oracle_agrees_with_reference_simulator() {
        let nb = Neighbourhood::new(2, vec![vec![1, 0], vec![0, 1], vec![1, 1]]).unwrap();
        let grid = Grid::new(&nb, 6);
        for seed in 0..300u64 {
            let rule = BitRule { k: 3, bits: crate::reference::splitmix64(seed) & 0x7FFF, mode: SurvivalMode::Free };
            let counts = crate::reference::reference_counts(&nb, &rule, 6);
            assert!(grid.matches(&rule, &counts));
            let mut wrong = counts.clone();
            wrong[6] += 1;
            assert!(!grid.matches(&rule, &wrong));
        }
    }

    // ------------------------------------------------------------------------------------
    // Larger spaces: no brute force possible, so check internal consistency instead.
    // ------------------------------------------------------------------------------------

    #[test]
    fn symmetry_only_merges_equivalent_rules_in_two_dimensions() {
        let nb = Neighbourhood::moore(2, 1).unwrap();
        assert_eq!(SymmetryGroup::preserving(&nb).order(), 8);
        for target in [vec![1, 2, 3, 5], vec![1, 2, 3, 4, 5], vec![1, 2, 3, 5, 7]] {
            for mode in ALL_MODES {
                let with = run(&nb, &target, mode, true, false);
                let without = run(&nb, &target, mode, false, false);
                assert_eq!(with.weighted, without.weighted, "{:?} {:?}", target, mode);
                assert!(with.nodes <= without.nodes);
                assert!(with.solutions <= without.solutions);
            }
        }
    }

    #[test]
    fn symmetry_group_of_order_48_in_three_dimensions() {
        // Octahedral group on the six axis neighbours, and the 26-neighbour cube with sparse tables.
        let octahedron = Neighbourhood::von_neumann(3).unwrap();
        assert_eq!(SymmetryGroup::preserving(&octahedron).order(), 48);
        for target in [vec![1, 2, 3, 4], vec![1, 2, 3, 5], vec![1, 2, 3, 5, 7]] {
            for mode in ALL_MODES {
                let with = run(&octahedron, &target, mode, true, false);
                let without = run(&octahedron, &target, mode, false, false);
                assert_eq!(with.weighted, without.weighted, "{:?} {:?}", target, mode);
                assert!(with.nodes * 3 < without.nodes || without.nodes < 50, "{:?} {:?}", target, mode);
            }
        }

        let cube = Neighbourhood::moore(3, 1).unwrap();
        assert_eq!(SymmetryGroup::preserving(&cube).order(), 48);
        let mut params = SearchParams::new(cube, vec![1, 2]);
        params.parallel = false;
        params.store_limit = 0;
        let with = search(&params).unwrap();
        params.symmetry = false;
        let without = search(&params).unwrap();
        // Either the seed survives and one of 26 neighbours is born, or it dies and two of 26 are.
        assert_eq!(without.solutions, 26 + 26 * 25 / 2);
        assert!(with.solutions < 30);
        assert_eq!(with.weighted, without.weighted);
    }

    #[test]
    fn parallel_search_equals_sequential_and_reports_progress() {
        let nb = Neighbourhood::moore(2, 1).unwrap();
        let target = vec![1, 2, 3, 5, 7];
        let progress = Progress::new();
        let mut params = SearchParams::new(nb.clone(), target.clone());
        params.threads = Some(4);
        params.store_limit = 0;
        params.progress = Some(progress.clone());
        let parallel = search(&params).unwrap();
        let sequential = run(&nb, &target, SurvivalMode::Free, true, false);
        assert_eq!(parallel.nodes_by_depth, sequential.nodes_by_depth);
        assert_eq!(parallel.solutions, sequential.solutions);
        assert_eq!(parallel.weighted, sequential.weighted);

        let json = progress.to_json();
        assert!(json.contains(&format!("\"nodes\":{}", parallel.nodes)), "{}", json);
        assert!(json.contains(&format!("\"solutions\":{}", parallel.solutions)), "{}", json);
        assert!(json.contains("\"running\":false"));
    }

    #[test]
    fn solutions_of_a_partial_search_are_sound() {
        let nb = Neighbourhood::moore(2, 1).unwrap();
        let target = primes_target(8);
        let mut params = SearchParams::new(nb.clone(), target.clone());
        params.parallel = false;
        params.store_limit = 40;
        params.node_limit = Some(200_000);
        let report = search(&params).unwrap();
        assert!(report.stored.len() > 10);
        for solution in &report.stored {
            assert!(verify_solution(&nb, &target, solution, SurvivalMode::Free, 6));
        }
    }

    #[test]
    fn entry_bound_is_a_pure_filter_on_the_solution_set() {
        let nb = Neighbourhood::moore(2, 1).unwrap();
        let target = vec![1, 2, 3, 5, 7];
        let unbounded = run(&nb, &target, SurvivalMode::Free, true, false);
        let total = SearchReport::space_bits(nb.len(), SurvivalMode::Free);
        for bound in [16u64, 18, 20, 24, 40] {
            let mut params = SearchParams::new(nb.clone(), target.clone());
            params.parallel = false;
            params.store_limit = 0;
            params.max_entries = Some(bound as usize);
            let bounded = search(&params).unwrap();
            let expected: BTreeMap<u64, u128> = unbounded
                .weighted
                .iter()
                .filter(|(&free, _)| total - free <= bound)
                .map(|(&free, &weight)| (free, weight))
                .collect();
            assert_eq!(bounded.weighted, expected, "bound {}", bound);
        }
    }

    #[test]
    fn find_first_stops_with_a_solution() {
        let nb = Neighbourhood::moore(2, 1).unwrap();
        let mut params = SearchParams::new(nb, primes_target(10));
        params.find_first = true;
        params.parallel = false;
        let report = search(&params).unwrap();
        assert_eq!(report.stop_reason, Some(StopReason::FoundFirst));
        assert!(report.solutions >= 1);
        assert_eq!(report.matched_terms(), 10);
    }

    #[test]
    fn node_limit_stops_a_runaway_search() {
        let nb = Neighbourhood::moore(2, 1).unwrap();
        let mut params = SearchParams::new(nb, primes_target(9));
        params.node_limit = Some(50_000);
        params.store_limit = 0;
        let report = search(&params).unwrap();
        assert_eq!(report.stop_reason, Some(StopReason::NodeLimit));
        assert!(report.nodes < 5_000_000);
    }

    #[test]
    fn single_seed_target_counts_every_rule() {
        let nb = Neighbourhood::moore(1, 1).unwrap();
        let report = run(&nb, &[1], SurvivalMode::Free, true, false);
        assert_eq!(report.solutions, 1);
        assert_eq!(report.exact_rule_count(), Some(1u128 << 7));
        assert!(search(&SearchParams::new(nb, vec![2, 3])).is_err());
    }

    #[test]
    fn sparse_and_dense_stores_agree() {
        let mut dense = Store::Dense(vec![0; 1 << 10]);
        let mut sparse = Store::Sparse(FxMap::default());
        for step in 0..5000u64 {
            let mask = crate::reference::splitmix64(step) & 0x3FF;
            let value = (crate::reference::splitmix64(step + 77) % 3) as u8;
            dense.set(mask, value);
            sparse.set(mask, value);
            let probe = crate::reference::splitmix64(step + 999) & 0x3FF;
            assert_eq!(dense.get(probe), sparse.get(probe));
        }
    }
}
