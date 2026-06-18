use mushroom_bot::search::search_best_move_report;

mod common;

#[derive(Clone, Copy)]
struct RunCase {
    name: &'static str,
    is_first: bool,
    board: fn() -> mushroom_bot::board::Board,
}

#[derive(Default)]
struct SideSummary {
    runs: i32,
    total_nodes: u64,
    total_elapsed_ms: i64,
    total_depth: i32,
}

impl SideSummary {
    fn record(&mut self, nodes: u64, elapsed_ms: i64, depth: i32) {
        self.runs += 1;
        self.total_nodes += nodes;
        self.total_elapsed_ms += elapsed_ms;
        self.total_depth += depth;
    }

    fn average_depth(&self) -> f64 {
        if self.runs == 0 {
            0.0
        } else {
            self.total_depth as f64 / self.runs as f64
        }
    }

    fn average_nps(&self) -> f64 {
        if self.total_elapsed_ms <= 0 {
            0.0
        } else {
            (self.total_nodes as f64 * 1000.0) / self.total_elapsed_ms as f64
        }
    }
}

#[test]
fn log_search_metrics_split_by_first_and_second() {
    let cases = [
        RunCase { name: "sparse", is_first: true, board: common::sparse_board },
        RunCase { name: "sparse", is_first: false, board: common::sparse_board },
        RunCase { name: "medium", is_first: true, board: common::medium_board },
        RunCase { name: "medium", is_first: false, board: common::medium_board },
        RunCase { name: "dense", is_first: true, board: common::dense_board },
        RunCase { name: "dense", is_first: false, board: common::dense_board },
    ];

    let mut first_summary = SideSummary::default();
    let mut second_summary = SideSummary::default();
    let mut overall_depth_total = 0i32;

    println!("SEARCH METRICS START");
    for case in cases {
        let board = (case.board)();
        let report = search_best_move_report(&board, 10_000, case.is_first);
        let nps = if report.elapsed_ms > 0 {
            report.nodes.saturating_mul(1000) / report.elapsed_ms as u64
        } else {
            0
        };
        let side = if case.is_first { "FIRST" } else { "SECOND" };

        println!(
            "case={} side={} elapsed_ms={} nodes={} nps={} depth={} best_move=({}, {}, {}, {})",
            case.name,
            side,
            report.elapsed_ms,
            report.nodes,
            nps,
            report.completed_depth,
            report.best_move.r1,
            report.best_move.c1,
            report.best_move.r2,
            report.best_move.c2,
        );

        assert!(report.elapsed_ms <= 10_000, "search exceeded budget on {} {}", case.name, side);
        assert!(report.completed_depth >= 1, "depth too shallow on {} {}", case.name, side);
        assert!(report.nodes > 0, "nodes should be positive on {} {}", case.name, side);

        if case.is_first {
            first_summary.record(report.nodes, report.elapsed_ms, report.completed_depth);
        } else {
            second_summary.record(report.nodes, report.elapsed_ms, report.completed_depth);
        }
        overall_depth_total += report.completed_depth;
    }

    let total_runs = (first_summary.runs + second_summary.runs) as f64;
    let average_depth_all = if total_runs > 0.0 {
        overall_depth_total as f64 / total_runs
    } else {
        0.0
    };

    println!(
        "SUMMARY side=FIRST runs={} total_nodes={} total_elapsed_ms={} avg_nps={:.2} avg_depth={:.2}",
        first_summary.runs,
        first_summary.total_nodes,
        first_summary.total_elapsed_ms,
        first_summary.average_nps(),
        first_summary.average_depth(),
    );
    println!(
        "SUMMARY side=SECOND runs={} total_nodes={} total_elapsed_ms={} avg_nps={:.2} avg_depth={:.2}",
        second_summary.runs,
        second_summary.total_nodes,
        second_summary.total_elapsed_ms,
        second_summary.average_nps(),
        second_summary.average_depth(),
    );
    println!("SUMMARY overall avg_depth={:.2}", average_depth_all);
    println!("SEARCH METRICS END");
}
