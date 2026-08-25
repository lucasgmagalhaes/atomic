use ipc::{FrameReader, FrameWriter};

fn unique_name(tag: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("nimble-ipc-test-{tag}-{nanos}")
}

#[test]
fn reader_sees_nothing_before_any_publish() {
    let name = unique_name("empty");
    let reader = FrameReader::new(&name, 4, 4).unwrap();
    assert_eq!(reader.generation(), 0);
    assert!(reader.latest_frame().is_none());
}

#[test]
fn reader_sees_the_published_frame() {
    let name = unique_name("basic");
    let mut writer = FrameWriter::new(&name, 2, 2).unwrap();
    let reader = FrameReader::new(&name, 2, 2).unwrap();

    let frame = vec![
        255u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 128, 128, 128, 255,
    ];
    writer.publish(&frame);

    assert_eq!(reader.generation(), 1);
    assert_eq!(reader.latest_frame().unwrap(), frame);
}

#[test]
fn generation_increments_per_publish() {
    let name = unique_name("gen");
    let mut writer = FrameWriter::new(&name, 1, 1).unwrap();
    let reader = FrameReader::new(&name, 1, 1).unwrap();

    writer.publish(&[1, 2, 3, 4]);
    assert_eq!(reader.generation(), 1);
    writer.publish(&[5, 6, 7, 8]);
    assert_eq!(reader.generation(), 2);
    assert_eq!(reader.latest_frame().unwrap(), vec![5, 6, 7, 8]);
}

#[test]
fn second_publish_never_corrupts_a_frame_already_copied_out() {
    // The core double-buffering guarantee: once latest_frame() has copied
    // a buffer, a subsequent publish() must not have retroactively
    // changed what was copied (it went into the *other* buffer).
    let name = unique_name("nocorrupt");
    let mut writer = FrameWriter::new(&name, 1, 1).unwrap();
    let reader = FrameReader::new(&name, 1, 1).unwrap();

    writer.publish(&[10, 10, 10, 255]);
    let first = reader.latest_frame().unwrap();
    writer.publish(&[20, 20, 20, 255]);

    assert_eq!(first, vec![10, 10, 10, 255]);
    assert_eq!(reader.latest_frame().unwrap(), vec![20, 20, 20, 255]);
}

#[test]
#[should_panic(expected = "size")]
fn publish_rejects_wrong_sized_buffer() {
    let name = unique_name("wrongsize");
    let mut writer = FrameWriter::new(&name, 4, 4).unwrap();
    writer.publish(&[0u8; 4]); // way too small for a 4x4 RGBA8 frame
}
