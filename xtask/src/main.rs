//! xtask stub (phase 1), plus a `bench-footprint` subcommand: spawns N
//! real `profile-worker` processes against a tiny local HTTP page and
//! reports per-process RAM/CPU, to answer (with real numbers) whether
//! process-per-tab is affordable at the "several accounts open at once"
//! scale this project targets. See plan Track A / spec/architecture/performance.md.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

const PAGE_HTML: &str = "<html><body><h1>xtask bench-footprint page</h1></body></html>";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("bench-footprint") => bench_footprint(),
        Some("check-neutron-boundary") => check_neutron_boundary(),
        _ => {
            println!("xtask stub (phase 1) — subcommands: bench-footprint, check-neutron-boundary")
        }
    }
}

/// Enforces the `neutron` encapsulation boundary (see
/// `spec/proposals/NEUTRON_ENCAPSULATION.md`): every crate outside
/// `crates/neutron/` must depend on the `neutron` facade only, never
/// path-dep directly into one of its sub-crates (`css`, `html`, `dom`,
/// `atoms`, `layout-engine`, `render`, `webgl`, `image_decode`,
/// `js-runtime`, `js-runtime/quickjs-sys`, `workers`). Wired into
/// `.cargo-husky/hooks/pre-commit` alongside the fmt/dprint checks.
fn check_neutron_boundary() {
    let sub_crates = [
        "css",
        "html",
        "dom",
        "atoms",
        "layout-engine",
        "render",
        "webgl",
        "image_decode",
        "js-runtime",
        "js-runtime/quickjs-sys",
        "workers",
    ];
    let repo_root = std::env::current_dir().expect("current_dir");
    let mut violations = Vec::new();
    for entry in walk_cargo_tomls(&repo_root) {
        if entry.starts_with(repo_root.join("crates").join("neutron")) {
            continue;
        }
        if entry.starts_with(repo_root.join("target")) {
            continue;
        }
        let text = std::fs::read_to_string(&entry).unwrap_or_default();
        for line in text.lines() {
            if !line.contains("path") || !line.contains('=') {
                continue;
            }
            for sub in sub_crates {
                let needle = format!("neutron/{sub}\"");
                if line.contains(&needle) {
                    violations.push(format!("{}: path dep into neutron/{sub}", entry.display()));
                }
            }
        }
    }
    if violations.is_empty() {
        println!("check-neutron-boundary: ok");
    } else {
        for v in &violations {
            eprintln!("check-neutron-boundary: {v}");
        }
        std::process::exit(1);
    }
}

fn walk_cargo_tomls(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == "target" || name == ".git" || name == "graphify-out" {
                continue;
            }
            out.extend(walk_cargo_tomls(&path));
        } else if path.file_name().and_then(|n| n.to_str()) == Some("Cargo.toml") {
            out.push(path);
        }
    }
    out
}

/// Finds the built `profile-worker` binary next to this xtask binary's own
/// target dir (same `debug`/`release` profile xtask itself was built with),
/// since both live under the same workspace `target/`.
fn worker_path() -> PathBuf {
    let mut dir = std::env::current_exe().expect("current_exe");
    dir.pop(); // drop xtask(.exe) filename, keep its containing target/<profile>/ dir
    let name = if cfg!(windows) {
        "profile-worker.exe"
    } else {
        "profile-worker"
    };
    let path = dir.join(name);
    assert!(
        path.exists(),
        "profile-worker not found at {path:?} — build it first: \
         cargo build -p profile --bin profile-worker (matching xtask's own --release/debug profile)"
    );
    path
}

/// Minimal one-page HTTP server: replies with the same fixed HTML to every
/// request, forever, on a background thread. Good enough as a fixed,
/// controlled navigation target for the footprint benchmark — it isn't
/// testing the HTTP stack, just giving every profile-worker something real
/// to fetch and render.
fn spawn_test_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local_addr");
    thread::spawn(move || {
        for stream in listener.incoming() {
            if let Ok(stream) = stream {
                handle_connection(stream);
            }
        }
    });
    format!("http://{addr}/")
}

fn handle_connection(mut stream: TcpStream) {
    let mut buf = [0u8; 1024];
    let _ = stream.read(&mut buf); // drain the request line(s); content is ignored
    let body = PAGE_HTML;
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
}

/// Spawns `n` real `profile-worker` processes, points each at `url`, waits
/// for first render, then samples per-process CPU/RAM twice (`sample_gap`
/// apart) to compute a CPU percentage and reports total + per-process
/// figures. Profiles are cleanly `quit()` afterward.
fn measure(n: usize, worker: &str, url: &str) {
    let sample_gap = Duration::from_millis(400);

    let mut profiles = Vec::new();
    for i in 0..n {
        let shmem_name = format!("xtask-bench-footprint-{i}");
        let mut p =
            profile::Profile::spawn(worker, &shmem_name, 800, 600).expect("spawn profile-worker");
        let _ = p.navigate(url);
        profiles.push(p);
    }

    // Let every profile finish its first fetch+layout+paint before sampling —
    // otherwise the "cold start, still fetching" window would understate
    // steady-state memory.
    thread::sleep(Duration::from_millis(800));

    let pids: Vec<u32> = profiles.iter().map(|p| p.pid()).collect();
    let before: Vec<_> = pids
        .iter()
        .map(|&pid| platform_apis::process_stats::sample(pid))
        .collect();
    let t0 = Instant::now();
    thread::sleep(sample_gap);
    let after: Vec<_> = pids
        .iter()
        .map(|&pid| platform_apis::process_stats::sample(pid))
        .collect();
    let elapsed = t0.elapsed();
    let cores = platform_apis::process_stats::logical_core_count();

    let mut total_mem = 0u64;
    let mut total_cpu_pct = 0.0f64;
    for (i, (b, a)) in before.iter().zip(after.iter()).enumerate() {
        match (b, a) {
            (Ok(b), Ok(a)) => {
                let cpu_pct = platform_apis::process_stats::cpu_percent(b, a, elapsed, cores);
                total_mem += a.memory_bytes;
                total_cpu_pct += cpu_pct;
                println!(
                    "  profile[{i}] pid={} mem={:.1} MiB cpu={:.1}%",
                    pids[i],
                    a.memory_bytes as f64 / (1024.0 * 1024.0),
                    cpu_pct
                );
            }
            _ => println!("  profile[{i}] pid={} sampling unsupported/failed", pids[i]),
        }
    }

    println!(
        "n={n} total_mem={:.1} MiB avg_mem_per_profile={:.1} MiB total_cpu={:.1}%",
        total_mem as f64 / (1024.0 * 1024.0),
        (total_mem as f64 / n as f64) / (1024.0 * 1024.0),
        total_cpu_pct
    );

    for p in profiles {
        p.quit();
    }
}

fn bench_footprint() {
    let worker = worker_path();
    let worker = worker.to_string_lossy().to_string();
    let url = spawn_test_server();
    // Give the server a moment to be definitely listening before the first
    // profile-worker tries to connect.
    thread::sleep(Duration::from_millis(100));

    println!("== bench-footprint: n=1 ==");
    measure(1, &worker, &url);
    println!("== bench-footprint: n=5 ==");
    measure(5, &worker, &url);
}
