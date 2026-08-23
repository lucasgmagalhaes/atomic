use js_runtime::{Context, Runtime};
use std::time::Duration;

fn pump_until<F: Fn(&Context) -> bool>(ctx: &Context, predicate: F, timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        ctx.run_pending_timers();
        if predicate(ctx) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
    }
}

#[test]
fn constructor_reads_back_sample_rate_and_length() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval("const c = new OfflineAudioContext(1, 100, 8000); `${c.sampleRate}|${c.length}`", "<test>")
        .unwrap();
    assert_eq!(result, "8000|100");
}

#[test]
fn an_unconnected_oscillator_renders_silence() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        r#"
        globalThis.seen = null;
        const c = new OfflineAudioContext(1, 8, 8000);
        const osc = c.createOscillator();
        osc.start(0); // never connected to destination
        c.startRendering().then((buf) => { seen = buf.getChannelData(0).every((s) => s === 0); });
        "#,
        "<test>",
    )
    .unwrap();

    let done = pump_until(&ctx, |c| c.eval("seen !== null", "<test>").unwrap() == "true", Duration::from_secs(2));
    assert!(done);
    assert_eq!(ctx.eval("seen", "<test>").unwrap(), "true");
}

#[test]
fn a_connected_oscillator_renders_a_real_sine_wave() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        r#"
        globalThis.samples = null;
        const c = new OfflineAudioContext(1, 4, 8000);
        const osc = c.createOscillator();
        osc.frequency = 1000;
        osc.connect(c.destination);
        osc.start(0);
        c.startRendering().then((buf) => { samples = buf.getChannelData(0).join(','); });
        "#,
        "<test>",
    )
    .unwrap();

    let done = pump_until(&ctx, |c| c.eval("samples !== null", "<test>").unwrap() == "true", Duration::from_secs(2));
    assert!(done);

    let samples_str = ctx.eval("samples", "<test>").unwrap();
    let got: Vec<f64> = samples_str.split(',').map(|s| s.parse().unwrap()).collect();
    let expected: Vec<f64> = (0..4).map(|i| (2.0 * std::f64::consts::PI * 1000.0 * (i as f64 / 8000.0)).sin()).collect();
    assert_eq!(got.len(), expected.len());
    for (g, e) in got.iter().zip(expected.iter()) {
        assert!((g - e).abs() < 1e-9, "got {g}, expected {e}");
    }
}

#[test]
fn gain_scales_the_oscillator_output() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        r#"
        globalThis.samples = null;
        const c = new OfflineAudioContext(1, 4, 8000);
        const osc = c.createOscillator();
        osc.frequency = 1000;
        const g = c.createGain();
        g.gain = 0.5;
        osc.connect(g);
        g.connect(c.destination);
        osc.start(0);
        c.startRendering().then((buf) => { samples = buf.getChannelData(0).join(','); });
        "#,
        "<test>",
    )
    .unwrap();

    let done = pump_until(&ctx, |c| c.eval("samples !== null", "<test>").unwrap() == "true", Duration::from_secs(2));
    assert!(done);

    let samples_str = ctx.eval("samples", "<test>").unwrap();
    let got: Vec<f64> = samples_str.split(',').map(|s| s.parse().unwrap()).collect();
    let expected: Vec<f64> = (0..4).map(|i| 0.5 * (2.0 * std::f64::consts::PI * 1000.0 * (i as f64 / 8000.0)).sin()).collect();
    for (g, e) in got.iter().zip(expected.iter()) {
        assert!((g - e).abs() < 1e-9, "got {g}, expected {e}");
    }
}

#[test]
fn stop_silences_the_oscillator_after_the_given_time() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        r#"
        globalThis.samples = null;
        // 8 samples at 8000Hz = 1ms total; stop at sample index 4 (t = 0.0005s).
        const c = new OfflineAudioContext(1, 8, 8000);
        const osc = c.createOscillator();
        osc.connect(c.destination);
        osc.start(0);
        osc.stop(0.0005);
        c.startRendering().then((buf) => { samples = buf.getChannelData(0).join(','); });
        "#,
        "<test>",
    )
    .unwrap();

    let done = pump_until(&ctx, |c| c.eval("samples !== null", "<test>").unwrap() == "true", Duration::from_secs(2));
    assert!(done);

    let samples_str = ctx.eval("samples", "<test>").unwrap();
    let got: Vec<f64> = samples_str.split(',').map(|s| s.parse().unwrap()).collect();
    assert_eq!(got.len(), 8);
    for s in &got[4..] {
        assert_eq!(*s, 0.0, "samples at/after stop() should be silent");
    }
    assert!(got[0..4].iter().any(|s| *s != 0.0), "samples before stop() should still be real audio");
}

#[test]
fn two_oscillators_mix_additively_at_the_destination() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        r#"
        globalThis.samples = null;
        const c = new OfflineAudioContext(1, 4, 8000);
        const a = c.createOscillator();
        a.frequency = 1000;
        a.connect(c.destination);
        a.start(0);
        const b = c.createOscillator();
        b.frequency = 2000;
        b.connect(c.destination);
        b.start(0);
        c.startRendering().then((buf) => { samples = buf.getChannelData(0).join(','); });
        "#,
        "<test>",
    )
    .unwrap();

    let done = pump_until(&ctx, |c| c.eval("samples !== null", "<test>").unwrap() == "true", Duration::from_secs(2));
    assert!(done);

    let samples_str = ctx.eval("samples", "<test>").unwrap();
    let got: Vec<f64> = samples_str.split(',').map(|s| s.parse().unwrap()).collect();
    let expected: Vec<f64> = (0..4)
        .map(|i| {
            let t = i as f64 / 8000.0;
            (2.0 * std::f64::consts::PI * 1000.0 * t).sin() + (2.0 * std::f64::consts::PI * 2000.0 * t).sin()
        })
        .collect();
    for (g, e) in got.iter().zip(expected.iter()) {
        assert!((g - e).abs() < 1e-9, "got {g}, expected {e}");
    }
}

#[test]
fn oscillator_type_defaults_to_sine_and_is_settable() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval(
            r#"
            const c = new OfflineAudioContext(1, 1, 8000);
            const osc = c.createOscillator();
            const before = osc.type;
            osc.type = 'square';
            `${before}|${osc.type}`
            "#,
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "sine|square");
}

#[test]
fn start_rendering_returns_a_real_promise() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx.eval("Object.prototype.toString.call(new OfflineAudioContext(1, 1, 8000).startRendering())", "<test>").unwrap();
    assert_eq!(result, "[object Promise]");
}
