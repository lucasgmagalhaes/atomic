use workers::Worker;

#[test]
fn onmessage_return_value_comes_back_through_the_channel() {
    let worker = Worker::spawn(
        "function onmessage(e) { return 'echo:' + e.data; }",
    );
    worker.post_message("hello");
    let reply = worker.recv_message().expect("worker should reply");
    assert_eq!(reply, "echo:hello");
    worker.terminate();
}

#[test]
fn worker_runs_real_js_computation_not_just_echo() {
    let worker = Worker::spawn(
        "function onmessage(e) { \
            var n = parseInt(e.data, 10); \
            var sum = 0; \
            for (var i = 1; i <= n; i++) sum += i; \
            return sum; \
        }",
    );
    worker.post_message("100");
    let reply = worker.recv_message().unwrap();
    assert_eq!(reply, "5050");
    worker.terminate();
}

#[test]
fn multiple_messages_are_handled_in_order() {
    let worker = Worker::spawn("function onmessage(e) { return e.data.toUpperCase(); }");
    worker.post_message("a");
    worker.post_message("b");
    worker.post_message("c");
    assert_eq!(worker.recv_message().unwrap(), "A");
    assert_eq!(worker.recv_message().unwrap(), "B");
    assert_eq!(worker.recv_message().unwrap(), "C");
    worker.terminate();
}

#[test]
fn worker_without_onmessage_replies_with_empty_string() {
    let worker = Worker::spawn("var x = 1;"); // no onmessage defined
    worker.post_message("anything");
    assert_eq!(worker.recv_message().unwrap(), "");
    worker.terminate();
}

#[test]
fn special_characters_in_message_are_escaped_safely() {
    let worker = Worker::spawn("function onmessage(e) { return e.data.length; }");
    worker.post_message("a\"b\\c\nd");
    let reply = worker.recv_message().unwrap();
    // a, ", b, \, c, \n, d = 7 characters once decoded back in JS.
    assert_eq!(reply, "7");
    worker.terminate();
}

#[test]
fn dropping_without_terminate_still_stops_the_thread() {
    let worker = Worker::spawn("function onmessage(e) { return e.data; }");
    worker.post_message("x");
    // Dropped here without calling terminate() - Drop must still join
    // the thread rather than leaking it.
}

#[test]
fn two_workers_run_independently() {
    let a = Worker::spawn("function onmessage(e) { return 'A:' + e.data; }");
    let b = Worker::spawn("function onmessage(e) { return 'B:' + e.data; }");
    a.post_message("x");
    b.post_message("y");
    assert_eq!(a.recv_message().unwrap(), "A:x");
    assert_eq!(b.recv_message().unwrap(), "B:y");
    a.terminate();
    b.terminate();
}
