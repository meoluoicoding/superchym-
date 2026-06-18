use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

#[derive(Clone, Copy, Debug)]
struct EvalWeights {
    territory: i32,
    mobility: i32,
    connectivity: i32,
    corners: i32,
    edges: i32,
    recapture: i32,
    vulnerability: i32,
}

#[derive(Clone, Copy, Debug)]
struct EvalWeightSet {
    first: EvalWeights,
    second: EvalWeights,
}

#[derive(Clone, Copy, Debug, Default)]
struct SideRecord {
    wins: i32,
    losses: i32,
    draws: i32,
}

#[derive(Clone, Copy, Debug, Default)]
struct MatchScore {
    overall: SideRecord,
    second: SideRecord,
}

impl MatchScore {
    fn objective(&self) -> f64 {
        let second_score = self.second.wins as f64
            - self.second.losses as f64
            + 0.5 * self.second.draws as f64;
        let overall_score = self.overall.wins as f64
            - self.overall.losses as f64
            + 0.25 * self.overall.draws as f64;
        second_score * 10.0 + overall_score
    }
}

#[derive(Clone, Debug)]
struct Config {
    iterations: u32,
    games: u32,
    seed: u64,
    time_budget_ms: u32,
    a: f64,
    c: f64,
    alpha: f64,
    gamma: f64,
    target_dir: String,
    exec1: String,
    exec2: String,
    log_dir: PathBuf,
    weights_path: PathBuf,
    best_weights_path: PathBuf,
    keep_logs: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            iterations: 20,
            games: 14,
            seed: 42,
            time_budget_ms: 10_000,
            a: 18.0,
            c: 8.0,
            alpha: 0.602,
            gamma: 0.101,
            target_dir: "target-codex".to_string(),
            exec1: r"C:\Users\khoa\Desktop\superchym\target-codex\release\mushroom_bot.exe".to_string(),
            exec2: r"C:\Users\khoa\Desktop\superchym\main4.exe".to_string(),
            log_dir: PathBuf::from("scripts"),
            weights_path: PathBuf::from("scripts/tune_second_current_weights.txt"),
            best_weights_path: PathBuf::from("scripts/tune_second_best_weights.txt"),
            keep_logs: false,
        }
    }
}

#[derive(Clone)]
struct SmallRng(u64);

impl SmallRng {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn plus_minus_one(&mut self) -> f64 {
        if (self.next_u64() & 1) == 0 { -1.0 } else { 1.0 }
    }
}

fn default_weights() -> EvalWeightSet {
    EvalWeightSet {
        first: EvalWeights {
            territory: 148,
            mobility: 20,
            connectivity: 19,
            corners: 18,
            edges: 3,
            recapture: 39,
            vulnerability: 9,
        },
        second: EvalWeights {
            territory: 140,
            mobility: 16,
            connectivity: 28,
            corners: 20,
            edges: 6,
            recapture: 28,
            vulnerability: 18,
        },
    }
}

fn parse_args() -> Config {
    let mut cfg = Config::default();
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--iterations" | "--iters" => {
                i += 1;
                cfg.iterations = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(cfg.iterations);
            }
            "--games" => {
                i += 1;
                cfg.games = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(cfg.games);
            }
            "--seed" => {
                i += 1;
                cfg.seed = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(cfg.seed);
            }
            "--time-budget" => {
                i += 1;
                cfg.time_budget_ms = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(cfg.time_budget_ms);
            }
            "--a" => {
                i += 1;
                cfg.a = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(cfg.a);
            }
            "--c" => {
                i += 1;
                cfg.c = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(cfg.c);
            }
            "--alpha" => {
                i += 1;
                cfg.alpha = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(cfg.alpha);
            }
            "--gamma" => {
                i += 1;
                cfg.gamma = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(cfg.gamma);
            }
            "--target-dir" => {
                i += 1;
                cfg.target_dir = args.get(i).cloned().unwrap_or_else(|| cfg.target_dir.clone());
            }
            "--exec1" => {
                i += 1;
                cfg.exec1 = args.get(i).cloned().unwrap_or_else(|| cfg.exec1.clone());
            }
            "--exec2" => {
                i += 1;
                cfg.exec2 = args.get(i).cloned().unwrap_or_else(|| cfg.exec2.clone());
            }
            "--weights-path" => {
                i += 1;
                cfg.weights_path = args.get(i).map(PathBuf::from).unwrap_or_else(|| cfg.weights_path.clone());
            }
            "--best-weights-path" => {
                i += 1;
                cfg.best_weights_path = args.get(i).map(PathBuf::from).unwrap_or_else(|| cfg.best_weights_path.clone());
            }
            "--log-dir" => {
                i += 1;
                cfg.log_dir = args.get(i).map(PathBuf::from).unwrap_or_else(|| cfg.log_dir.clone());
            }
            "--keep-logs" => {
                cfg.keep_logs = true;
            }
            _ => {}
        }
        i += 1;
    }
    cfg
}

fn make_absolute(path: &Path) -> std::io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn clamp_weight(index: usize, value: f64) -> i32 {
    let rounded = value.round() as i32;
    match index {
        4 => rounded.clamp(0, 40),
        _ => rounded.clamp(-300, 300),
    }
}

fn second_to_vec(weights: EvalWeights) -> [f64; 7] {
    [
        weights.territory as f64,
        weights.mobility as f64,
        weights.connectivity as f64,
        weights.corners as f64,
        weights.edges as f64,
        weights.recapture as f64,
        weights.vulnerability as f64,
    ]
}

fn vec_to_second(values: &[f64; 7]) -> EvalWeights {
    EvalWeights {
        territory: clamp_weight(0, values[0]),
        mobility: clamp_weight(1, values[1]),
        connectivity: clamp_weight(2, values[2]),
        corners: clamp_weight(3, values[3]),
        edges: clamp_weight(4, values[4]),
        recapture: clamp_weight(5, values[5]),
        vulnerability: clamp_weight(6, values[6]),
    }
}

fn write_weights_file(path: &Path, weights: &EvalWeightSet) -> std::io::Result<()> {
    let content = format!(
        "{} {} {} {} {} {} {} {} {} {} {} {} {} {}\n",
        weights.first.territory,
        weights.first.mobility,
        weights.first.connectivity,
        weights.first.corners,
        weights.first.edges,
        weights.first.recapture,
        weights.first.vulnerability,
        weights.second.territory,
        weights.second.mobility,
        weights.second.connectivity,
        weights.second.corners,
        weights.second.edges,
        weights.second.recapture,
        weights.second.vulnerability
    );
    fs::write(path, content)
}

fn parse_log(path: &Path) -> std::io::Result<MatchScore> {
    let content = fs::read_to_string(path)?;
    let mut side = String::new();
    let mut score = MatchScore::default();

    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("SIDE BOT1=") {
            side = rest.trim().to_string();
            continue;
        }
        let Some(rest) = line.strip_prefix("RESULT ") else {
            continue;
        };
        let result = rest.trim();
        let (overall_bucket, second_bucket) = if side == "SECOND" {
            (&mut score.overall, &mut score.second)
        } else {
            (&mut score.overall, &mut SideRecord::default())
        };

        if result == "DRAW" {
            overall_bucket.draws += 1;
            if side == "SECOND" {
                score.second.draws += 1;
            }
        } else if (side == "FIRST" && result == "FIRST") || (side == "SECOND" && result == "SECOND") {
            overall_bucket.wins += 1;
            if side == "SECOND" {
                second_bucket.wins += 1;
            }
        } else {
            overall_bucket.losses += 1;
            if side == "SECOND" {
                second_bucket.losses += 1;
            }
        }
    }

    Ok(score)
}

fn evaluate_weights(
    cfg: &Config,
    weights: &EvalWeightSet,
    trial_label: &str,
    seed: u64,
) -> std::io::Result<MatchScore> {
    write_weights_file(&cfg.weights_path, weights)?;
    let log_path = cfg.log_dir.join(format!("{trial_label}.log"));

    let output = Command::new("python")
        .arg(".\\scripts\\testing_tool.py")
        .args([
            "--exec1",
            &cfg.exec1,
            "--exec2",
            &cfg.exec2,
            "--games",
            &cfg.games.to_string(),
            "--shuffle-sides",
            "--seed",
            &seed.to_string(),
            "--time-budget",
            &cfg.time_budget_ms.to_string(),
            "--log",
            &log_path.to_string_lossy(),
        ])
        .env("EVAL_WEIGHTS_FILE", &cfg.weights_path)
        .output()?;

    if !output.status.success() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!(
                "testing_tool failed for {trial_label}: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    let parsed = parse_log(&log_path)?;
    if !cfg.keep_logs {
        let _ = fs::remove_file(&log_path);
    }
    Ok(parsed)
}

fn print_weights(label: &str, weights: EvalWeights) {
    eprintln!(
        "{label}: territory={} mobility={} connectivity={} corners={} edges={} recapture={} vulnerability={}",
        weights.territory,
        weights.mobility,
        weights.connectivity,
        weights.corners,
        weights.edges,
        weights.recapture,
        weights.vulnerability
    );
}

fn main() -> std::io::Result<()> {
    let mut cfg = parse_args();
    cfg.log_dir = make_absolute(&cfg.log_dir)?;
    cfg.weights_path = make_absolute(&cfg.weights_path)?;
    cfg.best_weights_path = make_absolute(&cfg.best_weights_path)?;
    fs::create_dir_all(&cfg.log_dir)?;

    let mut rng = SmallRng::new(cfg.seed ^ 0x9E37_79B9_7F4A_7C15);
    let mut current = default_weights();
    let mut best = current;
    let mut best_score = evaluate_weights(&cfg, &current, "tune_second_baseline", cfg.seed)?;
    let mut best_objective = best_score.objective();

    eprintln!("SPSA tuner for SECOND eval weights");
    eprintln!(
        "iterations={} games={} seed={} time_budget_ms={}",
        cfg.iterations, cfg.games, cfg.seed, cfg.time_budget_ms
    );
    print_weights("FIRST", current.first);
    print_weights("SECOND start", current.second);
    eprintln!(
        "baseline objective={:.2} overall={:?} second={:?}",
        best_objective, best_score.overall, best_score.second
    );

    let start = Instant::now();
    let mut theta = second_to_vec(current.second);

    for k in 0..cfg.iterations {
        let ak = cfg.a / ((k + 1) as f64).powf(cfg.alpha);
        let ck = cfg.c / ((k + 1) as f64).powf(cfg.gamma);
        let mut delta = [0.0; 7];
        for d in &mut delta {
            *d = rng.plus_minus_one();
        }

        let mut plus_vec = theta;
        let mut minus_vec = theta;
        for i in 0..7 {
            plus_vec[i] += ck * delta[i];
            minus_vec[i] -= ck * delta[i];
        }

        let plus_weights = EvalWeightSet { first: current.first, second: vec_to_second(&plus_vec) };
        let minus_weights = EvalWeightSet { first: current.first, second: vec_to_second(&minus_vec) };

        let plus_seed = cfg.seed + (k as u64) * 2 + 1;
        let minus_seed = cfg.seed + (k as u64) * 2 + 2;
        let plus_score = evaluate_weights(&cfg, &plus_weights, &format!("tune_second_plus_{k:03}"), plus_seed)?;
        let minus_score = evaluate_weights(&cfg, &minus_weights, &format!("tune_second_minus_{k:03}"), minus_seed)?;
        let y_plus = plus_score.objective();
        let y_minus = minus_score.objective();

        for i in 0..7 {
            let grad = (y_plus - y_minus) / (2.0 * ck * delta[i]);
            theta[i] -= ak * grad;
        }

        current.second = vec_to_second(&theta);
        let current_seed = cfg.seed + 10_000 + k as u64;
        let current_score = evaluate_weights(
            &cfg,
            &current,
            &format!("tune_second_curr_{k:03}"),
            current_seed,
        )?;
        let objective = current_score.objective();

        if objective > best_objective {
            best = current;
            best_score = current_score;
            best_objective = objective;
            write_weights_file(&cfg.best_weights_path, &best)?;
        }

        eprintln!(
            "iter {:>2}/{} objective={:>6.2} best={:>6.2} second={:?}",
            k + 1,
            cfg.iterations,
            objective,
            best_objective,
            current_score.second
        );
        print_weights("SECOND current", current.second);
    }

    write_weights_file(&cfg.best_weights_path, &best)?;
    let elapsed = start.elapsed().as_secs();
    eprintln!("done in {}s", elapsed);
    eprintln!(
        "best objective={:.2} overall={:?} second={:?}",
        best_objective, best_score.overall, best_score.second
    );
    print_weights("SECOND best", best.second);
    println!(
        "{} {} {} {} {} {} {}",
        best.second.territory,
        best.second.mobility,
        best.second.connectivity,
        best.second.corners,
        best.second.edges,
        best.second.recapture,
        best.second.vulnerability
    );

    Ok(())
}
