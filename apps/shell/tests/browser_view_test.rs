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
fn navigate_to_a_bad_url_records_a_navigation_error_without_losing_the_url() {
    let mut browser = BrowserView::spawn(200, 150);

    browser.navigate("not a url");
    assert_eq!(browser.current_url(), "not a url");
    assert!(browser.navigation_error().is_some());
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
