use shell::browser_view::{rgba_to_color_image, worker_binary_path, BrowserView};

#[test]
fn worker_binary_path_finds_the_real_profile_worker_next_to_this_test_binary() {
    // Real check, no mocking: this only passes if `profile-worker` was
    // actually built into the workspace's target directory (it will have
    // been, by anything that ran `cargo build --workspace` or built the
    // `profile` crate) - proves the sibling-binary lookup logic against
    // an actual file, not a fabricated path.
    let path = worker_binary_path().expect("profile-worker should be built alongside the workspace");
    assert!(path.exists());
    let file_name = path.file_name().unwrap().to_string_lossy();
    assert!(file_name.starts_with("profile-worker"));
}

#[test]
fn navigate_to_a_real_url_clears_any_navigation_error_and_records_the_url() {
    let mut browser = BrowserView::spawn(200, 150);
    assert!(browser.error().is_none(), "spawn should succeed against the real workspace build");

    browser.navigate("https://example.com/");
    assert_eq!(browser.current_url(), "https://example.com/");
    assert!(browser.navigation_error().is_none(), "navigating to a real URL should succeed");
}

#[test]
fn profile_mut_lends_the_real_running_profile() {
    let mut browser = BrowserView::spawn(200, 150);
    assert!(browser.error().is_none(), "spawn should succeed against the real workspace build");

    // Real proof this is the live profile, not a stand-in: a ping through
    // the borrowed handle actually reaches the spawned `profile-worker`.
    let profile = browser.profile_mut().expect("a successfully spawned view should have a live profile");
    assert!(profile.ping().expect("ping should reach the worker"));
}

#[test]
fn navigate_to_a_bad_url_records_a_navigation_error_without_losing_the_url() {
    let mut browser = BrowserView::spawn(200, 150);

    browser.navigate("not a url");
    assert_eq!(browser.current_url(), "not a url");
    assert!(browser.navigation_error().is_some());
}

/// A real forward proxy for tests: answers `CONNECT host:port HTTP/1.1`
/// with `200 Connection Established`, then splices raw bytes between the
/// client and `host:port` - a genuine tunnel, not a stand-in. Reports
/// each `CONNECT` target it handled on `seen`, so a test can prove
/// traffic actually went through it.
fn spawn_connect_proxy(seen: std::sync::mpsc::Sender<String>) -> u16 {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("proxy should bind");
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut client) = stream else { continue };
            let seen = seen.clone();
            std::thread::spawn(move || {
                let mut reader = BufReader::new(client.try_clone().expect("clone client stream"));
                let mut request_line = String::new();
                if reader.read_line(&mut request_line).is_err() || request_line.is_empty() {
                    return;
                }
                let target = request_line.trim_start_matches("CONNECT ").split(' ').next().unwrap_or("").to_string();
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) | Err(_) => return,
                        Ok(_) if line == "\r\n" => break,
                        Ok(_) => {}
                    }
                }
                let Ok(mut upstream) = std::net::TcpStream::connect(&target) else {
                    let _ = client.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n");
                    return;
                };
                let _ = seen.send(target);
                let _ = client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n");

                let mut upstream_reader = upstream.try_clone().expect("clone upstream stream");
                let mut client_writer = client.try_clone().expect("clone client stream");
                let client_to_upstream = std::thread::spawn(move || {
                    let _ = std::io::copy(&mut reader, &mut upstream);
                });
                let upstream_to_client = std::thread::spawn(move || {
                    let _ = std::io::copy(&mut upstream_reader, &mut client_writer);
                });
                let _ = client_to_upstream.join();
                let _ = upstream_to_client.join();
            });
        }
    });
    port
}

#[test]
fn spawn_with_proxy_routes_navigation_through_the_configured_proxy() {
    use std::io::{Read, Write};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("failed to bind a loopback test server");
    let page_addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let body = "<div>via shell proxy</div>";
            let response = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
            let _ = stream.write_all(response.as_bytes());
        }
    });

    let (seen_tx, seen_rx) = std::sync::mpsc::channel();
    let proxy_port = spawn_connect_proxy(seen_tx);

    let mut browser = BrowserView::spawn_with_proxy(64, 64, Some(&format!("127.0.0.1:{proxy_port}")));
    assert!(browser.error().is_none(), "spawn_with_proxy should succeed against the real workspace build");

    browser.navigate(&format!("http://{page_addr}/"));
    assert!(browser.navigation_error().is_none(), "navigating through the proxy should succeed: {:?}", browser.navigation_error());

    let tunneled_target = seen_rx.recv_timeout(std::time::Duration::from_secs(5)).expect("proxy should have handled a CONNECT for the page fetch");
    assert_eq!(tunneled_target, page_addr.to_string(), "the shell's profile should have tunneled through the proxy to the page's own address");
}

#[test]
fn rgba_to_color_image_preserves_dimensions_and_pixel_bytes() {
    let width = 2u32;
    let height = 2u32;
    let pixels: Vec<u8> = vec![
        255, 0, 0, 255, // red
        0, 255, 0, 255, // green
        0, 0, 255, 255, // blue
        255, 255, 0, 255, // yellow
    ];

    let image = rgba_to_color_image(&pixels, width, height);

    assert_eq!(image.size, [width as usize, height as usize]);
    assert_eq!(image.pixels.len(), 4);
    assert_eq!(image.pixels[0], egui::Color32::from_rgba_unmultiplied(255, 0, 0, 255));
    assert_eq!(image.pixels[1], egui::Color32::from_rgba_unmultiplied(0, 255, 0, 255));
    assert_eq!(image.pixels[2], egui::Color32::from_rgba_unmultiplied(0, 0, 255, 255));
    assert_eq!(image.pixels[3], egui::Color32::from_rgba_unmultiplied(255, 255, 0, 255));
}
