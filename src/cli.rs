use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crate::lattice::Neighbourhood;
use crate::reference::{
    pattern_to_string, reference_states, render_evolution, verify_solution, SolutionRule,
};
use crate::sequences::parse_target;
use crate::state_explorer::StateExplorer;
use crate::table_search::{search, SearchParams, SearchReport, Solution, StopReason, SurvivalMode};

const USAGE: &str = "\
CAS3 - cellular automata state space search for prime generation

USAGE:
    cas3 <COMMAND> [OPTIONS]

COMMANDS:
    search    Exact lazy rule-table search: does ANY rule (birth/survival tables over a
              neighbourhood) produce the target set-cell counts from a single seed cell?
    legacy    The original expression-tree enumeration (only practical in dimension 1)
    help      Show this message

OPTIONS (search):
    --dim N                 lattice dimension                          [default: 2]
    --neighbourhood KIND    moore | vonneumann                         [default: moore]
    --radius R              Moore radius                               [default: 1]
    --offsets LIST          custom neighbours, e.g. \"1,0;0,1;1,1\"
    --target SPEC           primes | naturals | odds | squares | powers2 | 1,2,3,5,...
                                                                       [default: primes]
    --terms N               number of terms for a named sequence       [default: 10]
    --survival MODE         free | always | never                      [default: free]
    --max-entries E         only rules consulting at most E table entries
    --no-symmetry           disable symmetry reduction (for measurements)
    --first                 stop at the first solution
    --show K                print up to K solutions                    [default: 3]
    --time-limit SECS       stop after this many seconds
    --node-limit N          stop after about N search nodes
    --threads N             worker threads                             [default: all cores]
    --sequential            run on a single thread

OPTIONS (legacy):
    --min-dim N  --max-dim N  --target SPEC  --terms N
";

struct Options {
    values: HashMap<String, String>,
    flags: HashSet<String>,
}

const FLAG_NAMES: [&str; 3] = ["no-symmetry", "first", "sequential"];

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
        self.values.get(name).cloned().unwrap_or_else(|| default.to_string())
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

fn build_neighbourhood(opts: &Options) -> Result<Neighbourhood, String> {
    let dim = opts.number::<usize>("dim")?.unwrap_or(2);
    if let Some(list) = opts.values.get("offsets") {
        let offsets = list
            .split(';')
            .map(|o| {
                o.split(',')
                    .map(|x| x.trim().parse::<i32>().map_err(|_| format!("bad offset '{}'", o)))
                    .collect::<Result<Vec<i32>, String>>()
            })
            .collect::<Result<Vec<_>, String>>()?;
        return Neighbourhood::new(dim, offsets);
    }
    match opts.text("neighbourhood", "moore").as_str() {
        "moore" => Neighbourhood::moore(dim, opts.number::<i32>("radius")?.unwrap_or(1)),
        "vonneumann" => Neighbourhood::von_neumann(dim),
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
    let target = parse_target(&opts.text("target", "primes"), terms)?;
    let mut params = SearchParams::new(nb, target);
    params.survival = parse_survival(&opts.text("survival", "free"))?;
    params.symmetry = !opts.flag("no-symmetry");
    params.find_first = opts.flag("first");
    params.parallel = !opts.flag("sequential");
    params.threads = opts.number::<usize>("threads")?;
    params.max_entries = opts.number::<usize>("max-entries")?;
    params.time_limit = opts.number::<f64>("time-limit")?.map(Duration::from_secs_f64);
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
    text.push_str(&format!("  symmetric images: {}\n", solution.orbit));
    text
}

fn run_search(opts: &Options) -> Result<(), String> {
    let params = build_params(opts)?;
    let nb = &params.neighbourhood;
    println!("neighbourhood : dim {} with {} neighbours", nb.dim, nb.len());
    println!("target        : {:?}", params.target);
    println!("survival      : {:?}", params.survival);
    println!(
        "unrestricted rule space: 2^{} tables",
        SearchReport::space_bits(nb.len(), params.survival)
    );

    let report = search(&params)?;

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
        print!("{}", describe_solution(nb, solution, params.survival));
        let rule = SolutionRule::new(solution, params.survival, None);
        let states = reference_states(nb, &rule, params.target.len() - 1);
        print!("{}", render_evolution(&states));
    }
    Ok(())
}

fn run_legacy(opts: &Options) -> Result<(), String> {
    let min_dim = opts.number::<usize>("min-dim")?.unwrap_or(1);
    let max_dim = opts.number::<usize>("max-dim")?.unwrap_or(1);
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
