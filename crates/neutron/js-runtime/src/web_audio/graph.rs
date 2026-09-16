//! The signal-flow graph (`NodeKind`/`GraphState`) + `node_output`, the
//! recursive evaluator — split out from `web_audio/mod.rs`.

/// Bounds `node_output`'s recursion so a feedback loop in the graph
/// renders as silence past the cap instead of hanging.
const MAX_GRAPH_DEPTH: u32 = 32;

pub(super) enum NodeKind {
    Destination,
    Oscillator {
        frequency: f64,
        wave_type: String,
        start: Option<f64>,
        stop: Option<f64>,
    },
    Gain {
        gain: f64,
    },
}

pub(super) struct GraphState {
    pub(super) sample_rate: f64,
    pub(super) length: usize,
    pub(super) channels: usize,
    pub(super) nodes: Vec<NodeKind>,
    pub(super) edges: Vec<(usize, usize)>,
}

/// Real recursive signal-flow evaluation: a node's output at time `t` is
/// a function of whatever feeds into it (sum of every edge's source,
/// scaled by gain for a `Gain` node, silence outside `[start, stop)` for
/// an `Oscillator`) — the same additive-mixing-at-merge-points model a
/// real Web Audio graph uses, just computed on demand per sample instead
/// of once per render quantum.
pub(super) fn node_output(graph: &GraphState, idx: usize, t: f64, depth: u32) -> f64 {
    if depth > MAX_GRAPH_DEPTH {
        return 0.0;
    }
    match &graph.nodes[idx] {
        NodeKind::Oscillator {
            frequency,
            start,
            stop,
            ..
        } => {
            let started = start.map(|s| t >= s).unwrap_or(true);
            let stopped = stop.map(|s| t >= s).unwrap_or(false);
            if started && !stopped {
                (2.0 * std::f64::consts::PI * frequency * t).sin()
            } else {
                0.0
            }
        }
        NodeKind::Gain { gain } => {
            let sum: f64 = graph
                .edges
                .iter()
                .filter(|(_, to)| *to == idx)
                .map(|(from, _)| node_output(graph, *from, t, depth + 1))
                .sum();
            sum * gain
        }
        NodeKind::Destination => graph
            .edges
            .iter()
            .filter(|(_, to)| *to == idx)
            .map(|(from, _)| node_output(graph, *from, t, depth + 1))
            .sum(),
    }
}
