use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use crate::analysis::{
    any_start_pairs, any_start_search, arity_scan_with, combine_profiles, complexity_profile,
    legacy_expression_variants, PairRun,
};
use crate::bounds::pair_has_central_cell;
use crate::construct::{
    prepared_prime_output_config, prime_mass_trace, BlockCodec, LocalRule, PreparedPrimeOutputRule,
    TAIL_BASE, TAIL_RESIDUES,
};
use crate::dashboard;
use crate::lattice::Neighbourhood;
use crate::mask_writer::SynchronizedPrimeOutput;
use crate::prime_worker::{
    CandidateBatchPipeline, CandidateResult, MeshCandidate, ParallelCandidate, PrimeWorker,
};
use crate::progress::Progress;
use crate::reference::{
    pattern_to_string, reference_states_from, render_evolution, seed_config, verify_solution,
    SolutionRule,
};
use crate::rule_class::{totalistic_notation, Classifier, RuleClass};
use crate::scheduler::{FifoResult, IntegratedCandidate, PrimeEpochPlan, RollingBatchField};
use crate::sequences::{parse_target, parse_target_from};
use crate::state_explorer::StateExplorer;
use crate::table_search::{search, SearchParams, SearchReport, Solution, StopReason, SurvivalMode};

const USAGE: &str = "\
CAS3 - cellular automata state space search for prime generation

USAGE:
    cargo run --release -- <COMMAND> [OPTIONS]

COMMANDS:
    search    Exact lazy rule-table search: does ANY rule (birth/survival tables over a
              neighbourhood) produce the target set-cell counts from a single seed cell
              (or from --seed, or from every starting configuration with --any-start)?
    profile   Smallest number of table entries a rule must consult to reproduce the first
              L terms, for every L. Flat curve = a real generator; steady growth = memorising.
    scan      Run `search` on every r-subset of the Moore neighbourhood (up to symmetry),
              r = 1..max-arity: all rules that depend on at most r neighbours.
    construct Verify the weighted split/tail recurrence for a constructive prime population
              machine and report the binary block budget needed by each prime gap.
    worker    Run the finite-state local trial-division CA on one binary candidate
    candidate Run a 2D parallel-divisor candidate tile with a local result reducer
    mesh      Run the compact n^(1/4)-diameter parallel candidate mesh
    pipeline  Diagnostic ordered stream of candidates classified by local CA tiles
    scheduler Run one mesh candidate and release it through the integrated local FIFO
    rolling   Recurrently classify stride-partitioned batches under one local CA rule
    epoch     Print the conservative dyadic coverage and population resource check
    mask      Decode one binary split count and commit a synchronized prime output
    primeca   Run the 2D prime-counting automaton (64-bit cells) from one set cell and check
              that the number of set cells is the next prime after every step
    primecols Run the 3D finite-alphabet version (numbers as digit columns) and check it
    sweep     Symmetric families (Life-like, von Neumann totalistic and isotropic) in any
              dimension up to 21, from every starting pattern: the longest run of primes
    space     Compare the legacy search-space size with the new one, per dimension
    legacy    The original expression-tree enumeration (only practical in dimension 1)
    help      Show this message

OPTIONS (search, profile, scan):
    --dim N                 lattice dimension                          [default: 2]
    --neighbourhood KIND    moore | vonneumann | hex (2D hexagonal lattice, axial
                            coordinates, with its 12 symmetries)       [default: moore]
    --radius R              Moore radius                               [default: 1]
    --offsets LIST          custom neighbours, e.g. \"1,0;0,1;1,1\"
    --target SPEC           primes | naturals | odds | squares | powers2 | 1,2,3,5,...
                                                                       [default: primes]
    --terms N               number of terms for a named sequence       [default: 10]
    --survival MODE         free | always | never                      [default: free]
    --class CLASS           table | isotropic | totalistic: which patterns share an entry
                            (isotropic = same after rotations/reflections, totalistic =
                            only the number of set neighbours counts, like Life)
                                                                       [default: table]
    --seed CELLS            start from these cells, e.g. \"-1,0;1,0\"; the target then
                            starts at the number of cells (primes from 2 for two cells)
    --any-start             search from every close pair of cells; by the pair reduction
                            this covers EVERY finite starting configuration for the
                            target 2, 3, 5, 7, ... (primes are aligned to start at 2)
    --max-entries E         only rules consulting at most E table entries
    --no-symmetry           disable symmetry reduction (for measurements)
    --first                 stop at the first solution
    --show K                print up to K solutions                    [default: 3]
    --time-limit SECS       stop after this many seconds (per attempt for profile)
    --node-limit N          stop after about N search nodes
    --threads N             worker threads                             [default: all cores]
    --sequential            run on a single thread
    --dashboard PORT        live monitor at http://127.0.0.1:PORT (0 = any free port)
    --hold                  keep the dashboard up after the run until Ctrl+C

OPTIONS (profile):
    --entry-cap E           give up beyond E consulted entries         [default: 80]

OPTIONS (scan):
    --min-arity R  --max-arity R   neighbour counts to scan            [default: 1..4]
    --universe-radius R     draw the neighbours from the Moore block of this radius
                                                                       [default: 1]
    --verbose               print every neighbour class

OPTIONS (construct):
    --steps N               number of prime-population transitions          [default: 100]
    --weight W              ON cells in each ordinary logical block         [default: 4]
    --show K                print up to K transitions                         [default: 8]

OPTIONS (worker):
    --candidate N           integer to classify                                [required]
    --max-steps N           local CA-step safety limit                  [default: 2000000]

OPTIONS (candidate):
    --candidate N           integer to classify                                [required]
    --max-steps N           local CA-step safety limit                    [default: 500000]

OPTIONS (mesh):
    --candidate N           integer to classify                                [required]
    --max-steps N           local CA-step safety limit                    [default: 500000]

OPTIONS (pipeline):
    --start N               first candidate to launch                          [default: 4]
    --lanes N               candidates in each autonomous counter batch       [default: 4]
    --steps N               pipeline scheduling cycles                         [default: 200]
    --show K                print up to K returned prime records                   [default: 8]

OPTIONS (scheduler):
    --candidate N           integer to classify and release                    [required]
    --max-steps N           local CA-step safety limit                    [default: 500000]

OPTIONS (rolling):
    --start N               first candidate in the first batch                 [default: 28]
    --lanes N               consecutive candidates per batch                    [default: 4]
    --stride N              candidate advance after each local recycle   [default: lanes]
    --rounds N              ordered batches to release                          [default: 3]
    --max-steps N           local CA-step safety limit                   [default: 2000000]

OPTIONS (epoch):
    --power K               certify the dyadic interval N=2^K                  [default: 88]

OPTIONS (mask):
    --splits K              ordinary four-cell births to synchronize             [default: 7]
    --old-residue R         current tail payload residue, 0..3                    [default: 1]
    --new-residue R         next tail payload residue, 0..3                       [default: 3]
    --max-steps N           local CA-step safety limit                     [default: 100000]

OPTIONS (sweep):
    --min-dim N  --max-dim N   dimensions to sweep                     [default: 2..8]
    --family F              life | life2 | vn | vn-iso | all           [default: all]
    --terms N               primes in the target, from 2               [default: 12]
    --node-limit N          search nodes per family and dimension      [default: 50000000]
    --max-offsets N         skip Moore families with more neighbours   [default: 200000]

OPTIONS (primeca):
    --steps N               steps to simulate and check                 [default: 1000]
    --report N              print every N steps                       [default: steps/10]
    --direct                apply the rule to every cell (default: event-driven)
    --certificate           timing certificate instead of a simulation
    --rows N                rows covered by the certificate      [default: every 64-bit row]

OPTIONS (primecols):
    --w W                   bits per digit, 3..64 (64 = the real automaton)      [default: 6]
    --k K                   head start: the clock reads step - K (0 = real)   [default: 8000]
    --rows N                rows simulated (rows never read the rows above)     [default: 30]
    --report N              print every N clock steps                  [default: about 10 lines]
    --certificate           timing certificate for every row (with --w, default 64)

OPTIONS (space):
    --max-dim N             largest dimension in the table             [default: 3]

OPTIONS (legacy):
    --min-dim N  --max-dim N  --target SPEC  --terms N
    --force                 attempt dimensions whose expression pairs do not fit in memory
";

struct Options {
    values: HashMap<String, String>,
    flags: HashSet<String>,
}

const FLAG_NAMES: [&str; 9] = [
    "no-symmetry",
    "first",
    "sequential",
    "hold",
    "verbose",
    "force",
    "any-start",
    "direct",
    "certificate",
];

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut values = HashMap::new();
    let mut flags = HashSet::new();
    let mut i = 0;
    while i < args.len() {
        let name = args[i]
            .strip_prefix("--")
            .ok_or_else(|| format!("unexpected argument '{}'", args[i]))?;
        if FLAG_NAMES.contains(&name) {
            flags.insert(name.to_string());
            i += 1;
        } else {
            let value = args
                .get(i + 1)
                .ok_or_else(|| format!("option --{} needs a value", name))?;
            values.insert(name.to_string(), value.clone());
            i += 2;
        }
    }
    Ok(Options { values, flags })
}

impl Options {
    fn text(&self, name: &str, default: &str) -> String {
        self.values
            .get(name)
            .cloned()
            .unwrap_or_else(|| default.to_string())
    }

    fn number<T: std::str::FromStr>(&self, name: &str) -> Result<Option<T>, String> {
        match self.values.get(name) {
            None => Ok(None),
            Some(v) => v
                .parse::<T>()
                .map(Some)
                .map_err(|_| format!("--{} expects a number, got '{}'", name, v)),
        }
    }

    fn flag(&self, name: &str) -> bool {
        self.flags.contains(name)
    }
}

fn parse_cell_list(list: &str) -> Result<Vec<Vec<i32>>, String> {
    list.split(';')
        .map(|o| {
            o.split(',')
                .map(|x| {
                    x.trim()
                        .parse::<i32>()
                        .map_err(|_| format!("bad offset '{}'", o))
                })
                .collect::<Result<Vec<i32>, String>>()
        })
        .collect()
}

fn build_neighbourhood(opts: &Options) -> Result<Neighbourhood, String> {
    let dim = opts.number::<usize>("dim")?.unwrap_or(2);
    if let Some(list) = opts.values.get("offsets") {
        return Neighbourhood::new(dim, parse_cell_list(list)?);
    }
    match opts.text("neighbourhood", "moore").as_str() {
        "moore" => Neighbourhood::moore(dim, opts.number::<i32>("radius")?.unwrap_or(1)),
        "vonneumann" => Neighbourhood::von_neumann(dim),
        "hex" if dim == 2 => Ok(Neighbourhood::hexagonal()),
        "hex" => Err("the hexagonal lattice needs --dim 2".to_string()),
        other => Err(format!("unknown neighbourhood '{}'", other)),
    }
}

fn parse_survival(text: &str) -> Result<SurvivalMode, String> {
    match text {
        "free" => Ok(SurvivalMode::Free),
        "always" => Ok(SurvivalMode::Always),
        "never" => Ok(SurvivalMode::Never),
        other => Err(format!("unknown survival mode '{}'", other)),
    }
}

fn build_params(opts: &Options) -> Result<SearchParams, String> {
    let nb = build_neighbourhood(opts)?;
    let terms = opts.number::<usize>("terms")?.unwrap_or(10);
    let seed = match opts.values.get("seed") {
        Some(list) => {
            let mut cells = parse_cell_list(list)?;
            cells.sort();
            cells.dedup();
            cells
        }
        None => vec![],
    };
    if opts.flag("any-start") && !seed.is_empty() {
        return Err("--any-start and --seed are exclusive".to_string());
    }
    let start = if opts.flag("any-start") {
        2
    } else {
        seed.len().max(1)
    };
    let target = parse_target_from(&opts.text("target", "primes"), terms, start)?;
    let mut params = SearchParams::new(nb, target);
    params.seed = seed;
    params.class = RuleClass::parse(&opts.text("class", "table"))?;
    if opts.text("neighbourhood", "moore") == "hex" && !opts.values.contains_key("offsets") {
        params.symmetry_matrices = crate::lattice::hexagonal_symmetries();
    }
    params.survival = parse_survival(&opts.text("survival", "free"))?;
    params.symmetry = !opts.flag("no-symmetry");
    params.find_first = opts.flag("first");
    params.parallel = !opts.flag("sequential");
    params.threads = opts.number::<usize>("threads")?;
    params.max_entries = opts.number::<usize>("max-entries")?;
    params.time_limit = opts
        .number::<f64>("time-limit")?
        .map(Duration::from_secs_f64);
    params.node_limit = opts.number::<u64>("node-limit")?;
    params.store_limit = opts.number::<usize>("show")?.unwrap_or(3);
    Ok(params)
}

fn format_count(report: &SearchReport) -> String {
    match (report.exact_rule_count(), report.log2_rule_count()) {
        (Some(n), _) => format!("{}", n),
        (None, Some(log2)) => format!("about 2^{:.1}", log2),
        _ => "0".to_string(),
    }
}

fn describe_solution(nb: &Neighbourhood, solution: &Solution, mode: SurvivalMode) -> String {
    if solution.classifier.class() == RuleClass::Totalistic {
        let survival = match mode {
            SurvivalMode::Free => Some(&solution.survival[..]),
            _ => None,
        };
        let mut text = format!(
            "  totalistic rule (set-neighbour counts): {}\n",
            totalistic_notation(nb.len(), &solution.birth, survival)
        );
        match mode {
            SurvivalMode::Always => text.push_str("  survival: set cells never die\n"),
            SurvivalMode::Never => text.push_str("  survival: set cells die after one step\n"),
            SurvivalMode::Free => {}
        }
        text.push_str(&format!("  seed: {:?}\n", solution.seed));
        return text;
    }
    let on = |entries: &[(u64, bool)]| -> Vec<String> {
        entries
            .iter()
            .filter(|(_, v)| *v)
            .map(|(m, _)| pattern_to_string(nb, *m))
            .collect()
    };
    let mut text = format!(
        "  birth    ON for {} of {} consulted patterns: {}\n",
        on(&solution.birth).len(),
        solution.birth.len(),
        on(&solution.birth).join(" ")
    );
    match mode {
        SurvivalMode::Free => text.push_str(&format!(
            "  survival ON for {} of {} consulted patterns: {}\n",
            on(&solution.survival).len(),
            solution.survival.len(),
            on(&solution.survival).join(" ")
        )),
        SurvivalMode::Always => text.push_str("  survival: set cells never die\n"),
        SurvivalMode::Never => text.push_str("  survival: set cells die after one step\n"),
    }
    if solution.classifier.class() == RuleClass::Isotropic {
        text.push_str(
            "  (isotropic: each pattern shown stands for all its rotations and reflections)\n",
        );
    }
    text.push_str(&format!(
        "  symmetric images: {}   seed: {:?}\n",
        solution.orbit, solution.seed
    ));
    text
}

/// Size of the rule class as "2^bits rules" (birth plus survival entries).
fn class_space(params: &SearchParams) -> Result<String, String> {
    let (nb, class, mode) = (&params.neighbourhood, params.class, params.survival);
    let classifier = Classifier::with_symmetries(class, nb, &params.symmetry_matrices)?;
    let entries = classifier.birth_entries()
        + if mode == SurvivalMode::Free {
            classifier.survival_entries()
        } else {
            0
        };
    let invariance = match class {
        RuleClass::Table => String::new(),
        RuleClass::Totalistic => ", only the number of set neighbours counts".to_string(),
        RuleClass::Isotropic => format!(
            ", the same under all {} symmetries of the neighbourhood",
            classifier.invariance_order()
        ),
    };
    Ok(format!(
        "2^{} rules ({} class, {} entries{})",
        entries,
        class.name(),
        entries,
        invariance
    ))
}

/// Starts the live dashboard when `--dashboard PORT` is given and returns the shared telemetry.
fn start_dashboard(opts: &Options) -> Result<Option<Arc<Progress>>, String> {
    let port = match opts.number::<u16>("dashboard")? {
        Some(port) => port,
        None => return Ok(None),
    };
    let progress = Progress::new();
    let bound = dashboard::serve(progress.clone(), port)
        .map_err(|e| format!("cannot start the dashboard on port {}: {}", port, e))?;
    println!("dashboard     : http://127.0.0.1:{}", bound);
    Ok(Some(progress))
}

fn hold_dashboard(opts: &Options, progress: &Option<Arc<Progress>>) {
    if progress.is_some() && opts.flag("hold") {
        println!("dashboard stays up; press Ctrl+C to exit");
        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    }
}

fn run_search(opts: &Options) -> Result<(), String> {
    let progress = start_dashboard(opts)?;
    let mut params = build_params(opts)?;
    params.progress = progress.clone();
    params.label = format!(
        "dim {} · {} neighbours · {} · {:?} survival",
        params.neighbourhood.dim,
        params.neighbourhood.len(),
        params.class.name(),
        params.survival
    );
    if opts.flag("any-start") {
        return run_any_start_search(opts, params, progress);
    }
    let nb = &params.neighbourhood;
    println!(
        "neighbourhood : dim {} with {} neighbours",
        nb.dim,
        nb.len()
    );
    println!("target        : {:?}", params.target);
    println!("survival      : {:?}", params.survival);
    if !params.seed.is_empty() {
        println!("seed          : {:?}", params.seed);
    }
    println!("rule space    : {}", class_space(&params)?);
    if let Some(term) =
        crate::bounds::first_light_cone_violation(nb.dim, nb.radius() as usize, &params.target)
    {
        println!(
            "note         : term {} of the target ({}) exceeds the light-cone limit of {}: no rule of this neighbourhood can reach it",
            term,
            params.target[term],
            crate::bounds::light_cone_limit(nb.dim, nb.radius() as usize, term) * params.target[0] as u128
        );
    }

    let report = search(&params)?;
    if let Some(p) = &progress {
        p.push_history(format!(
            "search finished: {} nodes, {} solution classes, longest prefix {} terms, {:.2?}",
            report.nodes,
            report.solutions,
            report.matched_terms(),
            report.elapsed
        ));
    }

    println!();
    println!("nodes per depth : {:?}", report.nodes_by_depth);
    println!("nodes visited   : {}", report.nodes);
    println!("elapsed         : {:.2?}", report.elapsed);
    println!(
        "longest matched prefix: {} of {} terms",
        report.matched_terms(),
        params.target.len()
    );
    println!(
        "solution classes: {}   (distinct rules matching: {})",
        report.solutions,
        format_count(&report)
    );
    match report.stop_reason {
        None => println!("search is exhaustive: the result is exact"),
        Some(StopReason::FoundFirst) => println!("stopped at the first solution"),
        Some(StopReason::TimeLimit) => println!("stopped at the time limit: result is partial"),
        Some(StopReason::NodeLimit) => println!("stopped at the node limit: result is partial"),
    }

    for (n, solution) in report.stored.iter().enumerate() {
        let verified = verify_solution(nb, &params.target, solution, params.survival, 16);
        println!();
        println!(
            "solution {} (re-verified with the reference simulator on 17 completions: {})",
            n + 1,
            if verified { "ok" } else { "FAILED" }
        );
        print_solution(nb, &params, solution);
    }
    hold_dashboard(opts, &progress);
    Ok(())
}

fn print_solution(nb: &Neighbourhood, params: &SearchParams, solution: &Solution) {
    print!("{}", describe_solution(nb, solution, params.survival));
    let rule = SolutionRule::new(solution, params.survival, None);
    let states = reference_states_from(nb, &rule, &seed_config(solution), params.target.len() - 1);
    print!("{}", render_evolution(&states));
}

/// `search --any-start`: every close pair, then the verdict for every starting configuration.
fn run_any_start_search(
    opts: &Options,
    params: SearchParams,
    progress: Option<Arc<Progress>>,
) -> Result<(), String> {
    let nb = params.neighbourhood.clone();
    let pairs = any_start_pairs(&params);
    println!(
        "neighbourhood : dim {} with {} neighbours",
        nb.dim,
        nb.len()
    );
    println!("rule space    : {}", class_space(&params)?);
    println!("target        : {:?}", params.target);
    println!(
        "any start     : a run 2, 3, ... from ANY configuration passes through a pair of cells closer than the reach of two neighbourhoods; {} pair shape(s) up to symmetry",
        pairs.len()
    );
    println!(
        "{:>14} {:>16} {:>9} {:>12} {:>14} {:>10}",
        "pair offset", "seed", "matched", "solutions", "nodes", "time"
    );
    let symmetric = params.class != RuleClass::Table;
    let runs = any_start_search(&params, |run: &PairRun| {
        let r = &run.report;
        println!(
            "{:>14} {:>16} {:>6}/{:<2} {:>12} {:>14} {:>10.2?}{}{}",
            format!("{:?}", run.offset),
            format!("{:?}", run.seed),
            r.matched_terms(),
            params.target.len(),
            r.solutions,
            r.nodes,
            r.elapsed,
            if r.complete() { "" } else { "  (limit)" },
            if symmetric && !pair_has_central_cell(&run.offset) {
                "  (no centre cell: the count stays even forever)"
            } else {
                ""
            }
        );
        if let Some(p) = &progress {
            p.push_history(format!(
                "pair {:?}: {} of {} terms, {} nodes{}",
                run.offset,
                r.matched_terms(),
                params.target.len(),
                r.nodes,
                if r.complete() { "" } else { " (limit)" }
            ));
        }
    })?;
    let best = runs
        .iter()
        .map(|r| r.report.matched_terms())
        .max()
        .unwrap_or(0);
    let exhaustive = runs.iter().all(|r| r.report.complete());
    println!();
    if exhaustive && best < params.target.len() {
        println!(
            "EXHAUSTIVE over every finite starting configuration: no {} rule of this neighbourhood ever has the counts {:?} in consecutive steps (longest run {} terms: {:?})",
            params.class.name(),
            &params.target[..(best + 1).min(params.target.len())],
            best,
            &params.target[..best]
        );
    } else if exhaustive {
        println!(
            "EXHAUSTIVE: some pair reproduces all {} target terms",
            params.target.len()
        );
    } else {
        println!(
            "partial (a limit was hit): the longest run found is {} terms, a lower bound",
            best
        );
    }
    if let Some(run) = runs
        .iter()
        .filter(|r| !r.report.stored.is_empty())
        .max_by_key(|r| r.report.matched_terms())
    {
        for (n, solution) in run.report.stored.iter().enumerate() {
            let verified = verify_solution(&nb, &params.target, solution, params.survival, 16);
            println!();
            println!(
                "solution {} from pair {:?} (re-verified on 17 completions: {})",
                n + 1,
                run.offset,
                if verified { "ok" } else { "FAILED" }
            );
            print_solution(&nb, &params, solution);
        }
    }
    if let Some(p) = &progress {
        p.finish(&format!(
            "any-start search {}: longest run {} of {} terms over {} pair shapes",
            if exhaustive { "exhaustive" } else { "partial" },
            best,
            params.target.len(),
            runs.len()
        ));
    }
    hold_dashboard(opts, &progress);
    Ok(())
}

fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn run_profile(opts: &Options) -> Result<(), String> {
    let progress = start_dashboard(opts)?;
    let mut params = build_params(opts)?;
    params.progress = progress.clone();
    params.label = format!(
        "profile · dim {} · {} neighbours · {:?} survival",
        params.neighbourhood.dim,
        params.neighbourhood.len(),
        params.survival
    );
    let last = params.target.len();
    if last < 2 {
        return Err("profile needs a target with at least 2 terms".to_string());
    }
    let entry_cap = opts.number::<usize>("entry-cap")?.unwrap_or(80);
    let attempt_limit = params.time_limit.take();
    params.max_entries = None;
    if opts.flag("any-start") {
        return run_any_start_profile(opts, params, entry_cap, attempt_limit, progress);
    }
    let nb = params.neighbourhood.clone();
    println!(
        "profile: dim {} with {} neighbours, survival {:?}, target {:?}",
        nb.dim,
        nb.len(),
        params.survival,
        params.target
    );
    println!("minimal number of table entries a rule must consult to reproduce the first L terms");
    println!(
        "{:>4} {:>6} {:>12} {:>5} {:>14} {:>10}",
        "L", "term", "min entries", "+", "nodes", "time"
    );

    let mut previous: Option<usize> = None;
    let rows = complexity_profile(&params, 2, last, entry_cap, attempt_limit, |row| {
        let term = params.target[row.terms - 1];
        let line = match row.min_entries {
            Some(entries) => {
                let delta = previous.map(|p| format!("{:+}", entries as i64 - p as i64)).unwrap_or_default();
                previous = Some(entries);
                format!(
                    "{:>4} {:>6} {:>12} {:>5} {:>14} {:>10.2?}",
                    row.terms, term, entries, delta, row.nodes, row.elapsed
                )
            }
            None => format!(
                "{:>4} {:>6} {:>12} {:>5} {:>14} {:>10.2?}   (all rules with < {} entries excluded; stopped by the cap or time limit)",
                row.terms,
                term,
                format!("> {}", row.ruled_out_below.saturating_sub(1)),
                "",
                row.nodes,
                row.elapsed,
                row.ruled_out_below
            ),
        };
        println!("{}", line);
        if let Some(p) = &progress {
            p.push_history(match row.min_entries {
                Some(entries) => format!(
                    "{} terms (…{}): minimal rule consults {} table entries · {} nodes · {:.1?}",
                    row.terms, term, entries, grouped(row.nodes), row.elapsed
                ),
                None => format!(
                    "{} terms (…{}): more than {} entries needed (cap or time limit) · {} nodes · {:.1?}",
                    row.terms, term, row.ruled_out_below.saturating_sub(1), grouped(row.nodes), row.elapsed
                ),
            });
            p.set_phase("profile", (row.terms - 1) as u64, (last - 1) as u64);
        }
    })?;

    if let Some(p) = &progress {
        let settled = rows.iter().filter(|r| r.min_entries.is_some()).count();
        let last_min = rows.iter().rev().find_map(|r| r.min_entries);
        p.finish(&format!(
            "profile finished: {} of {} prefix lengths settled exactly; the longest needs {} table entries",
            settled,
            last - 1,
            last_min.map(|m| m.to_string()).unwrap_or_else(|| "?".to_string())
        ));
    }

    if let Some(row) = rows.iter().rev().find(|r| r.solution.is_some()) {
        println!();
        println!(
            "simplest rule reproducing the first {} terms ({} table entries):",
            row.terms,
            row.min_entries.unwrap_or(0)
        );
        let solution = row.solution.as_ref().unwrap();
        let target = &params.target[..row.terms];
        let verified = verify_solution(&nb, target, solution, params.survival, 16);
        println!(
            "  re-verified with the reference simulator: {}",
            if verified { "ok" } else { "FAILED" }
        );
        let mut shown = params.clone();
        shown.target = target.to_vec();
        print_solution(&nb, &shown, solution);
    }
    hold_dashboard(opts, &progress);
    Ok(())
}

/// `profile --any-start`: the profile of every close pair and their minimum, which is the minimal
/// description length over every finite starting configuration (the pair itself is one offset).
fn run_any_start_profile(
    opts: &Options,
    params: SearchParams,
    entry_cap: usize,
    attempt_limit: Option<Duration>,
    progress: Option<Arc<Progress>>,
) -> Result<(), String> {
    let nb = params.neighbourhood.clone();
    let last = params.target.len();
    println!(
        "any-start profile: dim {} with {} neighbours, {} class, survival {:?}, target {:?}",
        nb.dim,
        nb.len(),
        params.class.name(),
        params.survival,
        params.target
    );
    let mut per_pair = vec![];
    for offset in any_start_pairs(&params) {
        let mut pair = params.clone();
        pair.seed = crate::bounds::pair_seed(&offset);
        pair.label = format!("{} · pair {:?}", params.label, offset);
        let mut line = format!("pair {:>12}:", format!("{:?}", offset));
        let rows = complexity_profile(&pair, 2, last, entry_cap, attempt_limit, |row| {
            match row.min_entries {
                Some(m) => line.push_str(&format!(" {}", m)),
                None => line.push_str(&format!(" >{}", row.ruled_out_below.saturating_sub(1))),
            }
        })?;
        println!("{}", line);
        if let Some(p) = &progress {
            p.push_history(line.clone());
        }
        per_pair.push(rows);
    }
    println!();
    println!(
        "minimal entries over every starting configuration (L = terms of {:?}):",
        params.target
    );
    println!("{:>4} {:>6} {:>12}", "L", "term", "min entries");
    for (terms, best, exact) in combine_profiles(&per_pair) {
        println!(
            "{:>4} {:>6} {:>12}",
            terms,
            params.target[terms - 1],
            match best {
                Some(b) if exact => b.to_string(),
                Some(b) => format!("<= {}", b),
                None => "?".to_string(),
            }
        );
    }
    if let Some(p) = &progress {
        p.finish("any-start profile finished");
    }
    hold_dashboard(opts, &progress);
    Ok(())
}

fn run_scan(opts: &Options) -> Result<(), String> {
    let progress = start_dashboard(opts)?;
    // The scan replaces the neighbourhood by subsets of the Moore block, so the base only needs the
    // dimension; the full Moore neighbourhood may be too large (80 neighbours in 4D).
    let mut base_values = opts.values.clone();
    if !base_values.contains_key("offsets") {
        base_values.insert("neighbourhood".to_string(), "vonneumann".to_string());
    }
    let base_opts = Options {
        values: base_values,
        flags: opts.flags.clone(),
    };
    let mut params = build_params(&base_opts)?;
    params.store_limit = 0;
    params.label = format!(
        "scan · dim {} · {:?} survival",
        params.neighbourhood.dim, params.survival
    );
    let min_arity = opts.number::<usize>("min-arity")?.unwrap_or(1);
    let max_arity = opts.number::<usize>("max-arity")?.unwrap_or(4);
    let universe_radius = opts.number::<i32>("universe-radius")?.unwrap_or(1);
    let dim = params.neighbourhood.dim;
    let verbose = opts.flag("verbose");
    println!(
        "scan: every rule depending on at most r neighbours drawn from the {}-dimensional Moore neighbourhood of radius {}, r = {}..={}",
        dim, universe_radius, min_arity, max_arity
    );
    println!("target {:?}, survival {:?}", params.target, params.survival);
    if let Some(term) =
        crate::bounds::first_light_cone_violation(dim, universe_radius as usize, &params.target)
    {
        println!(
            "note: term {} of the target ({}) exceeds what any rule of radius {} can reach in {} dimension(s) (light cone)",
            term, params.target[term], universe_radius, dim
        );
    }

    let any_start = opts.flag("any-start");
    if any_start {
        println!("every class is searched from each of its close pairs: the verdicts hold for every finite starting configuration");
    }
    let summaries = arity_scan_with(
        &params,
        min_arity..=max_arity,
        universe_radius,
        any_start,
        progress.clone(),
        |class| {
            if verbose {
                let offsets: Vec<String> = class
                    .offsets
                    .iter()
                    .map(|o| {
                        format!(
                            "({})",
                            o.iter()
                                .map(|x| format!("{:+}", x))
                                .collect::<Vec<_>>()
                                .join(",")
                        )
                    })
                    .collect();
                println!(
                "  r={} class {:>3}/{:<3} prefix {:>2} terms, {:>10} solution classes, {:>10} nodes {:>9.2?}{}  {}",
                class.arity,
                class.index + 1,
                class.classes_in_arity,
                class.matched_terms,
                class.solutions,
                class.nodes,
                class.elapsed,
                if class.complete { "" } else { " (limit)" },
                offsets.join(" ")
            );
            }
            if let Some(p) = &progress {
                if class.solutions > 0 || class.index + 1 == class.classes_in_arity {
                    p.push_history(format!(
                        "r={} class {}/{}: prefix {} terms, {} solution classes",
                        class.arity,
                        class.index + 1,
                        class.classes_in_arity,
                        class.matched_terms,
                        class.solutions
                    ));
                }
            }
        },
    )?;

    println!();
    println!(
        "{:>5} {:>8} {:>16} {:>12} {:>14} {:>10}  verdict",
        "r", "classes", "with solutions", "best prefix", "nodes", "time"
    );
    let total = params.target.len();
    for s in &summaries {
        let verdict = if s.incomplete > 0 {
            format!(
                "partial: {} classes hit a limit, prefix {} is a lower bound",
                s.incomplete, s.best_matched
            )
        } else if s.best_matched < total {
            format!(
                "DEAD: no rule with at most {} neighbours generates more than {} terms{}",
                s.arity,
                s.best_matched,
                if any_start { " from any start" } else { "" }
            )
        } else {
            format!("alive: some rule reproduces all {} target terms", total)
        };
        println!(
            "{:>5} {:>8} {:>16} {:>9}/{:<2} {:>14} {:>10.2?}  {}",
            s.arity,
            s.classes,
            s.with_solutions,
            s.best_matched,
            total,
            s.nodes,
            s.elapsed,
            verdict
        );
    }
    println!("best prefix = the most terms of the target that any rule with at most r neighbours can reproduce;");
    println!(
        "DEAD classes are proven: a rule generating the whole sequence would match every prefix."
    );
    if let Some(p) = &progress {
        let best = summaries.iter().map(|s| s.best_matched).max().unwrap_or(0);
        let cut_short: usize = summaries.iter().map(|s| s.incomplete).sum();
        let total = params.target.len();
        p.finish(&if cut_short > 0 {
            format!(
                "scan finished with {} classes cut short by a limit: best prefix found is {} of {} terms (a lower bound)",
                cut_short, best, total
            )
        } else if best < total {
            format!(
                "scan finished (exhaustive): no rule with at most {} neighbours reproduces more than {} of {} terms",
                max_arity, best, total
            )
        } else {
            format!(
                "scan finished (exhaustive): some rule with at most {} neighbours reproduces all {} terms",
                max_arity, total
            )
        });
    }
    hold_dashboard(opts, &progress);
    Ok(())
}

/// `primeca`: run the prime-counting automaton and check the number of set cells at every step.
fn run_primeca(opts: &Options) -> Result<(), String> {
    use crate::prime_display::{Simulation, BOOTSTRAP_LAST, BOOTSTRAP_ROWS};
    if opts.flag("certificate") {
        use crate::prime_display::{creation_step, last_row_in_64_bits, timing_certificate};
        let rows = opts
            .number::<u64>("rows")?
            .unwrap_or_else(last_row_in_64_bits);
        let started = std::time::Instant::now();
        println!(
            "timing certificate for rows {}..={} (every row after the table)",
            BOOTSTRAP_ROWS + 1,
            rows
        );
        match timing_certificate(rows) {
            Ok((row, margin)) => {
                println!(
                    "every tile learns its flip time in time; tightest bound: row {} with {:.1} steps to spare ({:.2?})",
                    row,
                    margin,
                    started.elapsed()
                );
                let overflow = creation_step(rows + 2);
                if rows == last_row_in_64_bits() {
                    println!(
                        "row {} is the first whose numbers overflow 64 bits; it appears at step {}, so the 64-bit rule is exact for every step before that",
                        rows + 2,
                        overflow
                    );
                } else {
                    println!("(row {} appears at step {})", rows + 2, overflow);
                }
            }
            Err(row) => return Err(format!("the bound fails at row {}", row)),
        }
        return Ok(());
    }
    let steps = opts.number::<u64>("steps")?.unwrap_or(1000);
    let every = opts.number::<u64>("report")?.unwrap_or((steps / 10).max(1));
    // Primes up to a bound safely above p_steps (p_t < t (ln t + ln ln t) for t >= 6).
    let t = steps.max(6) as f64;
    let limit = (t * (t.ln() + t.ln().ln()) + 100.0) as usize;
    let mut composite = vec![false; limit + 1];
    let mut primes = vec![];
    for m in 2..=limit {
        if !composite[m] {
            primes.push(m as u64);
            let mut k = m * m;
            while k <= limit {
                composite[k] = true;
                k += m;
            }
        }
    }
    println!(
        "prime-counting cellular automaton: 2D, Moore neighbourhood, one rule; seed = 1 set cell; flip times of n <= {} (rows <= {}) come from the rule's table, all larger n are computed by the cells",
        BOOTSTRAP_LAST, BOOTSTRAP_ROWS
    );
    let started = std::time::Instant::now();
    let direct = opts.flag("direct");
    println!(
        "simulation: {}",
        if direct {
            "direct (the rule applied to every cell of the triangle at every step)"
        } else {
            "by events (the rule evaluated wherever the state can change; cross-checked against the direct simulation in the tests)"
        }
    );
    let mut dense = if direct {
        Some(Simulation::new())
    } else {
        None
    };
    let mut events = if direct {
        None
    } else {
        Some(crate::prime_display::EventSimulation::new())
    };
    let count =
        |d: &Option<Simulation>, e: &Option<crate::prime_display::EventSimulation>| match (d, e) {
            (Some(d), _) => (d.set_count(), d.height()),
            (_, Some(e)) => (e.set_count(), e.height()),
            _ => unreachable!(),
        };
    println!(
        "{:>9} {:>12} {:>12} {:>8} {:>10}",
        "step", "set cells", "t-th prime", "rows", "time"
    );
    let (c0, h0) = count(&dense, &events);
    println!(
        "{:>9} {:>12} {:>12} {:>8} {:>10.2?}",
        0,
        c0,
        "(seed)",
        h0,
        started.elapsed()
    );
    for step in 1..=steps {
        if let Some(d) = dense.as_mut() {
            d.step();
        }
        if let Some(e) = events.as_mut() {
            e.step();
        }
        let (now, height) = count(&dense, &events);
        let expected = primes[(step - 1) as usize];
        if now != expected {
            return Err(format!(
                "step {}: {} set cells, but the {}-th prime is {}",
                step, now, step, expected
            ));
        }
        if step % every == 0 || step == steps {
            println!(
                "{:>9} {:>12} {:>12} {:>8} {:>10.2?}",
                step,
                now,
                expected,
                height,
                started.elapsed()
            );
        }
    }
    println!(
        "all {} steps match: the number of set cells is the next prime after every application of the rule",
        steps
    );
    let margin = match (&dense, &events) {
        (Some(d), _) => d.min_margin,
        (_, Some(e)) => e.min_margin,
        _ => None,
    };
    if let Some((margin, n)) = margin {
        println!(
            "tightest computed flip: cell {} learned its flip time {} steps early",
            n, margin
        );
    }
    Ok(())
}

/// `primecols`: the finite-alphabet 3D automaton, from one set cell, checked at every step.
fn run_primecols(opts: &Options) -> Result<(), String> {
    use crate::prime_columns::{wide_certificate, ColumnSim, Params};
    if opts.flag("certificate") {
        let w = opts.number::<u32>("w")?.unwrap_or(64);
        let started = std::time::Instant::now();
        println!(
            "timing certificate, {}-bit digits, no head start{}",
            w,
            if w == 64 { " (the real automaton)" } else { "" }
        );
        let c = wide_certificate(w)?;
        println!(
            "narrow rows 51..{}: every tile on time (2D certificate)",
            c.first_wide_row - 1
        );
        println!(
            "wide rows from {} (created at step {}): every column's countdown is exact before its deadline",
            c.first_wide_row, c.created
        );
        println!(
            "  checked on a grid up to row {:.0e}, tail estimate beyond; worst bound/deadline ratio {:.4} (near row {:.4e}) ({:.2?})",
            c.checked_to, c.worst_ratio, c.worst_row, started.elapsed()
        );
        return Ok(());
    }
    let w = opts.number::<u32>("w")?.unwrap_or(6);
    let k = opts.number::<u64>("k")?.unwrap_or(8000);
    let rows = opts.number::<u64>("rows")?.unwrap_or(30);
    if !(3..=64).contains(&w) || rows < 2 {
        return Err("need 3 <= --w <= 64 and --rows >= 2".into());
    }
    let params = Params { w, k };
    let last = (rows * (rows + 1) / 2) as usize;
    let mut composite = vec![false; last + 1];
    let mut pi = vec![0u64; last + 1];
    let mut primes = vec![];
    for j in 2..=last {
        if !composite[j] {
            primes.push(j as u64);
            let mut i = j * j;
            while i <= last {
                composite[i] = true;
                i += j;
            }
        }
        pi[j] = primes.len() as u64;
    }
    // The clock can run until the next prime lies beyond the simulated rows.
    let ticks = primes.len() as u64;
    let every = opts.number::<u64>("report")?.unwrap_or((ticks / 10).max(1));
    let narrow = (1..=rows).take_while(|&y| params.is_narrow(y)).count();
    println!(
        "prime-counting automaton with a finite alphabet: 3D, Moore neighbourhood, digits of {} bits, head start {} steps",
        w, k
    );
    println!(
        "rows 1..={} run the 2D rule in the plane z = 0; rows {}..={} store each number as a column of digits",
        narrow,
        narrow + 1,
        rows
    );
    println!(
        "checking: after step s the number of set cells is p_(s - {}) (p_0 = 1; none before), up to p = {}",
        k,
        primes.last().copied().unwrap_or(1)
    );
    println!(
        "{:>7} {:>10} {:>10} {:>6} {:>7} {:>12} {:>10}",
        "clock", "step", "set", "rows", "digits", "min slack", "time"
    );
    let started = std::time::Instant::now();
    let mut sim = ColumnSim::new(params, rows);
    let mut slack: Option<(i128, u128)> = None;
    let mut armed_at = HashMap::new();
    for step in 1..=k + ticks {
        sim.step();
        for &(x, y) in &sim.armed {
            armed_at.insert((x, y), step);
            if let Some((n, h)) = sim.column_number(x, y) {
                let due = (pi[(n - 1) as usize] + 1 + k) as i128;
                // The countdown is complete, its extra decrements delivered and their borrows
                // climbed to the top 3h steps after the bottom digit is set up.
                let margin = due - (step as i128 + 3 * h as i128);
                if slack.map_or(true, |(s, _)| margin < s) {
                    slack = Some((margin, n));
                }
            }
        }
        let expected = match step.checked_sub(k) {
            None => 0,
            Some(0) => 1,
            Some(t) => primes[t as usize - 1],
        };
        let now = sim.set_count();
        if now != expected {
            // Name the columns that are wrong, and whether their machinery was simply late.
            for y in 1..=rows as i64 {
                for x in 0..y {
                    let n = (y * (y - 1) / 2 + 1 + x) as usize;
                    let due = pi[n - 1] + 1 + k;
                    if sim.column_set(x, y) != (due <= step) {
                        println!(
                            "  n = {}: due at step {}, set = {}, countdown armed at {}; {}",
                            n,
                            due,
                            sim.column_set(x, y),
                            armed_at
                                .get(&(x, y))
                                .map_or("-".to_string(), |s| s.to_string()),
                            sim.column_status(x, y)
                        );
                    }
                }
            }
            return Err(format!(
                "step {}: {} set cells, expected {}",
                step, now, expected
            ));
        }
        if let Some(clock) = step.checked_sub(k) {
            if clock % every == 0 || clock == ticks {
                println!(
                    "{:>7} {:>10} {:>10} {:>6} {:>7} {:>12} {:>10.2?}",
                    clock,
                    step,
                    now,
                    sim.height(),
                    sim.depth(),
                    slack.map_or("-".to_string(), |(s, _)| s.to_string()),
                    started.elapsed()
                );
            }
        }
    }
    println!(
        "all {} steps match: the count is p_(s - {}) after every step s",
        k + ticks,
        k
    );
    if let Some((margin, n)) = slack {
        println!(
            "tightest column: n = {} was ready {} steps before its deadline",
            n, margin
        );
    }
    Ok(())
}

/// `sweep`: symmetric families from every starting pattern, dimension by dimension.
fn run_sweep(opts: &Options) -> Result<(), String> {
    use crate::dimension_sweep::{sweep, Family, MAX_SWEEP_DIM};
    let min_dim = opts.number::<usize>("min-dim")?.unwrap_or(2);
    let max_dim = opts
        .number::<usize>("max-dim")?
        .unwrap_or(8)
        .min(MAX_SWEEP_DIM);
    let terms = opts.number::<usize>("terms")?.unwrap_or(12);
    let node_limit = opts.number::<u64>("node-limit")?.unwrap_or(50_000_000);
    let max_offsets = opts.number::<usize>("max-offsets")?.unwrap_or(200_000);
    let families: Vec<Family> = match opts.text("family", "all").as_str() {
        "life" => vec![Family::MooreTotalistic(1)],
        "life2" => vec![Family::MooreTotalistic(2)],
        "vn" => vec![Family::VonNeumannTotalistic],
        "vn-iso" => vec![Family::VonNeumannIsotropic],
        "all" => vec![
            Family::MooreTotalistic(1),
            Family::MooreTotalistic(2),
            Family::VonNeumannTotalistic,
            Family::VonNeumannIsotropic,
        ],
        other => {
            return Err(format!(
                "unknown family '{}' (life | life2 | vn | vn-iso | all)",
                other
            ))
        }
    };
    let target = parse_target_from("primes", terms, 2)?;
    println!(
        "sweep: symmetric families from EVERY finite starting pattern (centred pairs, see bounds.rs), target {:?}",
        target
    );
    println!(
        "{:>4}  {:<30} {:>14} {:>6} {:>8}  {:<26} {:>12} {:>10}",
        "dim", "family", "entries", "pairs", "run", "longest run of primes", "nodes", "time"
    );
    for dim in min_dim..=max_dim {
        for &family in &families {
            let neighbours = ((2 * match family {
                Family::MooreTotalistic(r) => r,
                _ => 1,
            } + 1) as f64)
                .powi(dim as i32);
            if matches!(family, Family::MooreTotalistic(_)) && neighbours > max_offsets as f64 {
                continue;
            }
            let result = sweep(family, dim, &target, node_limit)?;
            let shown: Vec<String> = target[..result.best_run]
                .iter()
                .map(|p| p.to_string())
                .collect();
            println!(
                "{:>4}  {:<30} {:>14} {:>6} {:>5}/{:<2}  {:<26} {:>12} {:>10.2?}{}",
                result.dim,
                result.family.name(),
                result.entries,
                result.pairs,
                result.best_run,
                result.terms_checked,
                shown.join(","),
                result.nodes,
                result.elapsed,
                if result.complete {
                    ""
                } else {
                    "  (node limit: lower bound)"
                }
            );
        }
    }
    Ok(())
}

fn run_space(opts: &Options) -> Result<(), String> {
    let max_dim = opts.number::<usize>("max-dim")?.unwrap_or(3);
    println!(
        "{:>3} {:>7} {:>28} {:>26} {:>26}",
        "dim",
        "inputs",
        "legacy structures / function",
        "legacy (birth, survival)",
        "unrestricted tables"
    );
    for dim in 1..=max_dim {
        let inputs = (3usize).pow(dim as u32);
        let neighbours = inputs - 1;
        let structures = legacy_expression_variants(inputs);
        let pairs = structures.checked_mul(structures);
        let tables = if neighbours <= 62 {
            format!(
                "2^{}",
                SearchReport::space_bits(neighbours, SurvivalMode::Free)
            )
        } else {
            "not supported".to_string()
        };
        println!(
            "{:>3} {:>7} {:>28} {:>26} {:>26}",
            dim,
            inputs,
            structures,
            pairs
                .map(|p| format!("{}", p))
                .unwrap_or_else(|| "> 3.4e38".to_string()),
            tables
        );
    }

    println!("legacy structures exclude the input permutations tried for every structure;");
    println!("the new engine never enumerates tables: it branches only on entries a simulation consults.");
    Ok(())
}

fn run_construct(opts: &Options) -> Result<(), String> {
    let steps = opts.number::<usize>("steps")?.unwrap_or(100);
    let weight = opts.number::<usize>("weight")?.unwrap_or(4);
    let show = opts.number::<usize>("show")?.unwrap_or(8);
    let trace = prime_mass_trace(steps, weight)?;
    let output_steps = if weight == 4 {
        let codec = BlockCodec::with_tail_states(TAIL_BASE, TAIL_RESIDUES)?;
        let rule = PreparedPrimeOutputRule;
        trace
            .iter()
            .filter(|transition| transition.before.ordinary_blocks > 0)
            .try_fold(0usize, |verified, transition| {
                let current = prepared_prime_output_config(transition, weight)?;
                if codec.logical_weight(&current)? != transition.before.population {
                    return Err("prepared output pattern has the wrong population".to_string());
                }
                let next = rule.step(&current)?;
                if codec.logical_weight(&next)? != transition.after.population {
                    return Err(format!(
                        "local output gate failed at prime transition {}",
                        transition.index
                    ));
                }
                Ok(verified + 1)
            })?
    } else {
        0
    };
    println!("constructive population certificate (not a prime scheduler):");
    println!("  dimension       : 2");
    println!("  transitions     : {}", trace.len());
    println!("  ordinary weight : {} ON cells per block", weight);
    println!("  first population: 1");
    println!(
        "  last population : {}",
        trace.last().map(|s| s.after.population).unwrap_or(1)
    );
    println!();
    println!(
        "{:>5} {:>9} {:>9} {:>7} {:>10} {:>10} {:>9}",
        "t", "before", "after", "gap", "blocks->", "tail->", "tail Δ"
    );
    for step in trace.iter().take(show) {
        println!(
            "{:>5} {:>9} {:>9} {:>7} {:>4}->{:<5} {:>4}->{:<5} {:>+9}",
            step.index,
            step.before.population,
            step.after.population,
            step.prime_gap,
            step.before.ordinary_blocks,
            step.after.ordinary_blocks,
            step.before.tail_weight,
            step.after.tail_weight,
            step.tail_delta
        );
    }
    if trace.len() > show {
        println!(
            "  ... {} more exact transitions omitted",
            trace.len() - show
        );
    }
    let max_gap = trace.iter().map(|s| s.prime_gap).max().unwrap_or(0);
    println!();
    println!(
        "mass invariant: exact for every displayed and omitted transition; largest prime gap = {}",
        max_gap
    );
    if weight == 4 {
        println!(
            "local split/tail gate: exact for all {} post-bootstrap transitions",
            output_steps
        );
    } else {
        println!("local split/tail gate: available only for the implemented frame weight 4");
    }
    println!("scheduler status: still a separate local finite-state proof obligation; finite trace only checks population accounting");
    Ok(())
}

fn run_worker(opts: &Options) -> Result<(), String> {
    let candidate = opts
        .number::<usize>("candidate")?
        .ok_or_else(|| "worker needs --candidate N".to_string())?;
    let max_steps = opts.number::<usize>("max-steps")?.unwrap_or(2_000_000);
    let mut worker = PrimeWorker::new(candidate)?;
    let result = worker.run_until_result(max_steps)?;
    println!("candidate     : {}", candidate);
    println!("logical cells : {}", worker.config.len());
    println!("local steps   : {}", worker.steps);
    println!("CA result     : {:?}", result);
    println!("active states : finite binary n/d/s/r tape plus one control head");
    Ok(())
}

fn run_candidate(opts: &Options) -> Result<(), String> {
    let candidate = opts
        .number::<usize>("candidate")?
        .ok_or_else(|| "candidate needs --candidate N".to_string())?;
    let max_steps = opts.number::<usize>("max-steps")?.unwrap_or(500_000);
    let rows = ((candidate as f64).sqrt().floor() as usize).saturating_sub(1);
    let mut tile = ParallelCandidate::new(candidate)?;
    let result = tile.run_until_result(max_steps)?;
    println!("candidate       : {}", candidate);
    println!("parallel rows   : {}", rows);
    println!("logical cells   : {}", tile.config.len());
    println!("local steps     : {}", tile.steps);
    println!("CA result       : {:?}", result);
    println!("classification : local remainder workers + vertical finite-state reducer");
    Ok(())
}

fn run_mesh(opts: &Options) -> Result<(), String> {
    let candidate = opts
        .number::<usize>("candidate")?
        .ok_or_else(|| "mesh needs --candidate N".to_string())?;
    let max_steps = opts.number::<usize>("max-steps")?.unwrap_or(500_000);
    let mut tile = MeshCandidate::new(candidate)?;
    let result = tile.run_until_result(max_steps)?;
    println!("candidate       : {}", candidate);
    println!(
        "mesh shape      : {} columns x {} rows",
        tile.columns, tile.rows
    );
    println!("logical cells   : {}", tile.config.len());
    println!("local steps     : {}", tile.steps);
    println!("CA result       : {:?}", result);
    println!("classification : row/column local AND wave over parallel divisor tapes");
    Ok(())
}

fn run_pipeline(opts: &Options) -> Result<(), String> {
    let start = opts.number::<usize>("start")?.unwrap_or(4);
    let lanes = opts.number::<usize>("lanes")?.unwrap_or(4);
    let steps = opts.number::<usize>("steps")?.unwrap_or(200);
    let show = opts.number::<usize>("show")?.unwrap_or(8);
    let mut pipeline = CandidateBatchPipeline::new(start, lanes)?;
    let mut records = Vec::new();
    for _ in 0..steps {
        records.extend(pipeline.step()?);
    }
    let primes: Vec<_> = records
        .iter()
        .filter(|record| record.result == CandidateResult::Prime)
        .collect();
    println!("ordered stream is a diagnostic harness, not a one-seed population CA");
    println!("scheduling cycles : {}", steps);
    println!("batch lanes        : {}", lanes);
    println!("released candidates: {}", records.len());
    println!("returned primes    : {}", primes.len());
    println!();
    println!("{:>7} {:>10} {:>8}", "prime", "launch", "CA steps");
    for record in primes.iter().take(show) {
        println!(
            "{:>7} {:>10} {:>8}",
            record.candidate,
            record.launched_at,
            record.completed_at - record.launched_at
        );
    }
    println!();
    for pair in primes.windows(2).take(show) {
        println!(
            "gap {:>5} -> {:>5} = {}",
            pair[0].candidate,
            pair[1].candidate,
            pair[1].candidate - pair[0].candidate
        );
    }
    Ok(())
}

fn run_scheduler(opts: &Options) -> Result<(), String> {
    let candidate = opts
        .number::<usize>("candidate")?
        .ok_or_else(|| "scheduler needs --candidate N".to_string())?;
    let max_steps = opts.number::<usize>("max-steps")?.unwrap_or(500_000);
    let mut scheduler = IntegratedCandidate::new(candidate)?;
    let result = scheduler.run_until_release(max_steps)?;
    scheduler.settle_gap()?;
    println!("candidate       : {}", candidate);
    println!("logical cells   : {}", scheduler.config.len());
    println!("local steps     : {}", scheduler.steps);
    println!("FIFO release    : {:?}", result);
    println!("release pulses  : {}", scheduler.gap_value()?);
    println!("classification and ordered release: one synchronous tagged CA rule");
    Ok(())
}

fn run_rolling(opts: &Options) -> Result<(), String> {
    let start = opts.number::<usize>("start")?.unwrap_or(28);
    let lanes = opts.number::<usize>("lanes")?.unwrap_or(4);
    let stride = opts.number::<usize>("stride")?.unwrap_or(lanes);
    let rounds = opts.number::<usize>("rounds")?.unwrap_or(3);
    let max_steps = opts.number::<usize>("max-steps")?.unwrap_or(2_000_000);
    let mut field = RollingBatchField::new_with_stride(start, lanes, stride)?;
    field.run_until_batches(rounds, max_steps)?;
    println!("recurrent ordered candidate batches (one finite-state local rule):");
    println!("  lanes          : {}", lanes);
    println!("  lane stride    : {}", stride);
    println!("  batches        : {}", field.batches.len());
    println!("  local steps    : {}", field.steps);
    println!("  logical cells  : {}", field.config.len());
    for (round, batch) in field.batches.iter().enumerate() {
        let summary = batch
            .iter()
            .map(|(candidate, result)| {
                format!(
                    "{}{}",
                    candidate,
                    if *result == FifoResult::Prime {
                        "p"
                    } else {
                        "c"
                    }
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        println!("  batch {:>3}     : {}", round, summary);
    }
    println!("host code only observes the released stream; classification, backpressure,");
    println!("stride advance, tape growth, reset, and lane ordering are local CA transitions.");
    Ok(())
}

fn run_epoch(opts: &Options) -> Result<(), String> {
    let power = opts.number::<u32>("power")?.unwrap_or(88);
    let plan = PrimeEpochPlan::for_power(power)?;
    println!("dyadic recurrent-scheduler resource check:");
    println!("  interval floor       : 2^{}", plan.power);
    println!("  candidate lanes      : {}", plan.lanes);
    println!("  candidate word bits  : {}", plan.word_bits);
    println!("  worker latency bound : {:.3e}", plan.worker_latency);
    println!("  ring families        : {:.3e}", plan.ring_families);
    println!("  ring logical blocks  : {:.3e}", plan.ring_blocks);
    println!("  packed gap blocks    : {:.3e}", plan.gap_buffer_blocks);
    println!(
        "  scheduler live cells : {:.3e} ({:.6} of N)",
        plan.scheduler_live_cells, plan.scheduler_fraction
    );
    println!(
        "  candidate capacity   : {:.6} x next dyadic interval",
        plan.candidate_capacity
    );
    println!(
        "  admitted coverage    : {:.6} x next dyadic interval",
        plan.candidate_coverage
    );
    println!("  admitted batches     : {:.3e}", plan.admitted_batches);
    println!("  admission / epoch    : {:.6}", plan.admission_fraction);
    println!("  max gap / N          : {:.8}", plan.maximum_gap_fraction);
    println!("  mask latency bound   : {:.3e}", plan.mask_latency);
    println!("  mask lead / epoch    : {:.6}", plan.mask_lead_fraction);
    println!("  queue latency bound  : {:.3e}", plan.queue_latency);
    println!("  queue lead / epoch   : {:.12}", plan.queue_lead_fraction);
    println!(
        "  builder blocks / gen : {:.6}",
        plan.builder_blocks_per_generation
    );
    println!("  builder / lane share : {:.6}", plan.builder_lane_fraction);
    println!(
        "  next-ring births / N : {:.6}",
        plan.next_epoch_build_fraction()?
    );
    println!(
        "  next-epoch reserve   : {:.6}",
        plan.next_epoch_reserve_fraction()?
    );
    println!("  numeric checks pass  : {}", plan.certified());
    println!(
        "  first checked K      : {}",
        PrimeEpochPlan::first_certified_power()?
    );
    println!("this f64 check covers throughput, mass, mask decoding, and queue timing;");
    println!("exact all-t inequalities, slot insertion, and epoch construction remain.");
    Ok(())
}

fn run_mask(opts: &Options) -> Result<(), String> {
    let splits = opts.number::<usize>("splits")?.unwrap_or(7);
    let old_residue = opts.number::<usize>("old-residue")?.unwrap_or(1);
    let new_residue = opts.number::<usize>("new-residue")?.unwrap_or(3);
    let max_steps = opts.number::<usize>("max-steps")?.unwrap_or(100_000);
    let width = (usize::BITS - splits.max(1).leading_zeros()) as usize + 1;
    let mut output = SynchronizedPrimeOutput::new(splits, width, splits, old_residue, new_residue)?;
    let before = output.binary_weight()?;
    output.run_until_ready(max_steps)?;
    let ready = output.binary_weight()?;
    output.commit()?;
    let after = output.binary_weight()?;
    let expected = (4 * splits) as isize + new_residue as isize - old_residue as isize;
    let actual = after as isize - ready as isize;
    if before != ready || actual != expected {
        return Err(format!(
            "synchronized output changed by {}, expected {}; preparation {} -> {}",
            actual, expected, before, ready
        ));
    }
    println!("local synchronized prime-output gate:");
    println!("  split parents     : {}", splits);
    println!("  tail residue      : {} -> {}", old_residue, new_residue);
    println!("  preparation steps : {}", output.steps - 1);
    println!("  binary population : {} -> {}", ready, after);
    println!("  exact delta       : {:+}", actual);
    println!("binary record loading, prefix writing, firing-squad synchronization,");
    println!("ordinary births, and tail adjustment are all local CA transitions.");
    Ok(())
}

/// The legacy search keeps every (birth, survival) expression pair in memory.
const LEGACY_PAIR_LIMIT: u128 = 100_000_000;

fn legacy_guard(max_dim: usize, force: bool) -> Result<(), String> {
    if force {
        return Ok(());
    }
    for dim in 1..=max_dim {
        let structures = legacy_expression_variants((3usize).pow(dim as u32));
        let pairs = structures.checked_mul(structures).unwrap_or(u128::MAX);
        if pairs > LEGACY_PAIR_LIMIT {
            return Err(format!(
                "the legacy search at dimension {} has to hold {} expression pairs in memory (see the `space` command). \
Use the `search` command instead, or pass --force to try anyway",
                dim,
                if pairs == u128::MAX { "more than 3.4e38".to_string() } else { grouped_u128(pairs) }
            ));
        }
    }
    Ok(())
}

fn grouped_u128(n: u128) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn run_legacy(opts: &Options) -> Result<(), String> {
    let min_dim = opts.number::<usize>("min-dim")?.unwrap_or(1);
    let max_dim = opts.number::<usize>("max-dim")?.unwrap_or(1);
    legacy_guard(max_dim, opts.flag("force"))?;
    let terms = opts.number::<usize>("terms")?.unwrap_or(10);
    let target = parse_target(&opts.text("target", "primes"), terms)?;
    let explorer = StateExplorer::new(min_dim, max_dim, target);
    // Birth/Survival rule-set search (cells can die, so primes are possible).
    // `explore()` is the older single-rule search for monotonic sequences.
    explorer.explore_rule_sets();
    Ok(())
}

pub fn run() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("help");
    let result = match command {
        "search" => parse_options(&args[1..]).and_then(|o| run_search(&o)),
        "profile" => parse_options(&args[1..]).and_then(|o| run_profile(&o)),
        "scan" => parse_options(&args[1..]).and_then(|o| run_scan(&o)),
        "construct" => parse_options(&args[1..]).and_then(|o| run_construct(&o)),
        "worker" => parse_options(&args[1..]).and_then(|o| run_worker(&o)),
        "candidate" => parse_options(&args[1..]).and_then(|o| run_candidate(&o)),
        "mesh" => parse_options(&args[1..]).and_then(|o| run_mesh(&o)),
        "pipeline" => parse_options(&args[1..]).and_then(|o| run_pipeline(&o)),
        "scheduler" => parse_options(&args[1..]).and_then(|o| run_scheduler(&o)),
        "rolling" => parse_options(&args[1..]).and_then(|o| run_rolling(&o)),
        "epoch" => parse_options(&args[1..]).and_then(|o| run_epoch(&o)),
        "mask" => parse_options(&args[1..]).and_then(|o| run_mask(&o)),
        "space" => parse_options(&args[1..]).and_then(|o| run_space(&o)),
        "sweep" => parse_options(&args[1..]).and_then(|o| run_sweep(&o)),
        "primeca" => parse_options(&args[1..]).and_then(|o| run_primeca(&o)),
        "primecols" => parse_options(&args[1..]).and_then(|o| run_primecols(&o)),
        "legacy" => parse_options(&args[1..]).and_then(|o| run_legacy(&o)),
        "help" | "--help" | "-h" => {
            print!("{}", USAGE);
            Ok(())
        }
        other => Err(format!("unknown command '{}'\n\n{}", other, USAGE)),
    };
    if let Err(message) = result {
        eprintln!("error: {}", message);
        std::process::exit(2);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn options_split_into_values_and_flags() {
        let opts = parse_options(&args(&[
            "--dim", "3", "--first", "--target", "1,2,3", "--hold",
        ]))
        .unwrap();
        assert_eq!(opts.number::<usize>("dim").unwrap(), Some(3));
        assert_eq!(opts.text("target", "primes"), "1,2,3");
        assert!(opts.flag("first") && opts.flag("hold") && !opts.flag("sequential"));
        assert!(parse_options(&args(&["--dim"])).is_err());
        assert!(parse_options(&args(&["dim", "3"])).is_err());
        assert!(parse_options(&args(&["--dim", "x"]))
            .unwrap()
            .number::<usize>("dim")
            .is_err());
    }

    #[test]
    fn search_parameters_have_sensible_defaults() {
        let params = build_params(&parse_options(&[]).unwrap()).unwrap();
        assert_eq!(params.neighbourhood.dim, 2);
        assert_eq!(params.neighbourhood.len(), 8);
        assert_eq!(params.target, vec![1, 2, 3, 5, 7, 11, 13, 17, 19, 23]);
        assert_eq!(params.survival, SurvivalMode::Free);
        assert!(params.symmetry && params.parallel && !params.find_first);

        let custom = build_params(
            &parse_options(&args(&[
                "--dim",
                "2",
                "--offsets",
                "1,0;0,1;1,1",
                "--survival",
                "always",
                "--no-symmetry",
                "--max-entries",
                "30",
                "--time-limit",
                "1.5",
                "--target",
                "naturals",
                "--terms",
                "5",
            ]))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(custom.neighbourhood.len(), 3);
        assert_eq!(custom.survival, SurvivalMode::Always);
        assert!(!custom.symmetry);
        assert_eq!(custom.max_entries, Some(30));
        assert_eq!(custom.time_limit, Some(Duration::from_millis(1500)));
        assert_eq!(custom.target, vec![1, 2, 3, 4, 5]);

        let vn = build_params(
            &parse_options(&args(&["--dim", "3", "--neighbourhood", "vonneumann"])).unwrap(),
        )
        .unwrap();
        assert_eq!(vn.neighbourhood.len(), 6);
        assert!(
            build_params(&parse_options(&args(&["--survival", "sometimes"])).unwrap()).is_err()
        );
        assert!(
            build_params(&parse_options(&args(&["--neighbourhood", "triangle"])).unwrap()).is_err()
        );

        let hex = build_params(
            &parse_options(&args(&["--neighbourhood", "hex", "--class", "isotropic"])).unwrap(),
        )
        .unwrap();
        assert_eq!(
            (hex.neighbourhood.len(), hex.symmetry_matrices.len()),
            (6, 12)
        );
        assert_eq!(hex.class, RuleClass::Isotropic);
        assert!(build_params(
            &parse_options(&args(&["--dim", "3", "--neighbourhood", "hex"])).unwrap()
        )
        .is_err());
        assert!(build_params(&parse_options(&args(&["--class", "fancy"])).unwrap()).is_err());

        let pair =
            build_params(&parse_options(&args(&["--seed", "1,0;-1,0", "--terms", "4"])).unwrap())
                .unwrap();
        assert_eq!(pair.seed, vec![vec![-1, 0], vec![1, 0]]);
        assert_eq!(pair.target, vec![2, 3, 5, 7]);
        let any =
            build_params(&parse_options(&args(&["--any-start", "--terms", "3"])).unwrap()).unwrap();
        assert!(any.seed.is_empty());
        assert_eq!(any.target, vec![2, 3, 5]);
        assert!(build_params(
            &parse_options(&args(&["--any-start", "--seed", "0,0;2,0"])).unwrap()
        )
        .is_err());
    }

    #[test]
    fn legacy_search_is_refused_where_it_cannot_fit_in_memory() {
        assert!(legacy_guard(1, false).is_ok());
        let refused = legacy_guard(2, false).unwrap_err();
        assert!(refused.contains("37,950,331,027,456"), "{}", refused);
        assert!(legacy_guard(3, false).is_err());
        assert!(legacy_guard(2, true).is_ok());
    }

    #[test]
    fn numbers_are_grouped_in_thousands() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1_000), "1,000");
        assert_eq!(grouped(171_397_094), "171,397,094");
        assert_eq!(grouped_u128(37_950_331_027_456), "37,950,331,027,456");
    }
}
